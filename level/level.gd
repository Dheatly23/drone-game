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

@onready var chunks := $Chunks

var buffer_data := PackedByteArray()
var buffer_offset := 0

func set_buffer(buf: PackedByteArray) -> void:
	buffer_data = buf
	buffer_offset = 0

func _ready() -> void:
	wasm_instance.call_wasm(&"init", [chunk_size_x, chunk_size_y, chunk_size_z])

	var scene := preload("res://level/chunk/chunk.tscn")
	for z in range(chunk_size_z):
		for y in range(chunk_size_y):
			for x in range(chunk_size_x):
				var node := scene.instantiate()
				node.coord_x = x
				node.coord_y = y
				node.coord_z = z
				node.name = "Chunk_%d_%d_%d" % [x, y, z]
				chunks.add_child(node)

func _tick() -> void:
	for i in range(chunks.get_child_count()):
		chunks.get_child(i).update_chunk(wasm_instance)

func __wasm_read_buffer(p: int, n: int) -> int:
	if n + buffer_offset >= len(buffer_data):
		var b := buffer_data.slice(buffer_offset)
		wasm_instance.memory_write(p, b)
		buffer_data = PackedByteArray()
		buffer_offset = 0
		return len(b)
	else:
		wasm_instance.memory_write(p, buffer_data.slice(buffer_offset, n + buffer_offset))
		buffer_offset += n
		return n

func __wasm_write_buffer(p: int, n: int) -> void:
	buffer_data.append_array(wasm_instance.memory_read(p, n))

func __wasm_log(p: int, n: int) -> void:
	print(wasm_instance.memory_read(p, n).get_string_from_utf8())
