extends Node3D

@export var wasm_module: WasmModule = null
@export_group("Chunk Size")
@export_range(1, 64) var chunk_size_x := 1
@export_range(1, 64) var chunk_size_y := 1
@export_range(1, 64) var chunk_size_z := 1

@onready var wasm_instance := WasmInstance.new().initialize(
	wasm_module,
	{
		"host": {
			"log": {
				params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
				results = [],
				callable = __wasm_log,
			},
			"read_data": {
				params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I64],
				results = [WasmHelper.TYPE_I64],
				callable = __wasm_read_buffer,
			},
			"write_data": {
				params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I64],
				results = [],
				callable = __wasm_write_buffer,
			},
		},
	},
	{
		"epoch.enable": true,
		"epoch.timeout": 1.0,
	},
)

var chunks := {}

var buffer_data := PackedByteArray()

func init_chunks() -> void:
	var old_size := Vector3i(chunk_size_x, chunk_size_y, chunk_size_z)

	chunk_size_x = wasm_instance.call_wasm(&"get_chunk_x", [])[0]
	chunk_size_y = wasm_instance.call_wasm(&"get_chunk_y", [])[0]
	chunk_size_z = wasm_instance.call_wasm(&"get_chunk_z", [])[0]

	for k: Vector3i in chunks.keys():
		if k.min(old_size) != k:
			chunks[k].queue_free()
			chunks.erase(k)

	var scene := preload("res://level/chunk/chunk.tscn")
	var parent := $Chunks
	for z in range(chunk_size_z):
		for y in range(chunk_size_y):
			for x in range(chunk_size_x):
				var coord := Vector3i(x, y, z)
				if coord in chunks:
					continue

				var node := scene.instantiate()
				node.coord_x = x
				node.coord_y = y
				node.coord_z = z
				node.name = "Chunk_%d_%d_%d" % [x, y, z]
				parent.add_child(node)
				chunks[coord] = node

	update_chunks()

func update_chunks() -> void:
	for k in chunks:
		chunks[k].update_chunk(wasm_instance)

func init_empty() -> void:
	wasm_instance.call_wasm(&"init", [chunk_size_x, chunk_size_y, chunk_size_z])
	init_chunks()

func import_level(data: PackedByteArray) -> void:
	buffer_data = data
	wasm_instance.call_wasm(&"import", [])
	init_chunks()

func tick() -> void:
	update_chunks()

func __wasm_read_buffer(p: int, n: int) -> int:
	if len(buffer_data) > n :
		wasm_instance.signal_error("Buffer is insufficient")
		return 0
	wasm_instance.memory_write(p, buffer_data)
	return len(buffer_data)

func __wasm_write_buffer(p: int, n: int) -> void:
	buffer_data = wasm_instance.memory_read(p, n)

func __wasm_log(p: int, n: int) -> void:
	print(wasm_instance.memory_read(p, n).get_string_from_utf8())
