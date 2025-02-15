extends Node3D

signal chunks_updated()

@export var wasm_module: WasmModule = null
@export_group("Chunk Size")
@export_range(1, 64) var chunk_size_x := 1
@export_range(1, 64) var chunk_size_y := 1
@export_range(1, 64) var chunk_size_z := 1

@onready var wasm_instance := WasmInstance.new().initialize(
	wasm_module,
	{
		"host": {
			"random": {
				params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
				results = [],
				callable = __wasm_random,
			},
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
			"entity_removed": {
				params = [
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
				],
				results = [],
				callable = __wasm_entity_removed,
			},
			"entity_iron_ore": {
				params = [
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I64,
				],
				results = [],
				callable = __wasm_entity_iron_ore,
			},
			"entity_drone": {
				params = [
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
				],
				results = [],
				callable = __wasm_entity_drone,
			},
		},
	},
	{
		"epoch.enable": true,
		"epoch.timeout": 5.0,
	},
)

var chunks := {}
var block_entities := {}

var buffer_data := PackedByteArray()
var crypto := Crypto.new()
var mutex := Mutex.new()

func init_chunks() -> void:
	var old_size := Vector3i(chunk_size_x, chunk_size_y, chunk_size_z)

	mutex.lock()
	chunk_size_x = wasm_instance.call_wasm(&"get_chunk_x", [])[0]
	chunk_size_y = wasm_instance.call_wasm(&"get_chunk_y", [])[0]
	chunk_size_z = wasm_instance.call_wasm(&"get_chunk_z", [])[0]
	mutex.unlock()

	for k: Vector3i in chunks.keys():
		if k.min(old_size) != k:
			chunks[k].queue_free()
			chunks.erase(k)
		else:
			chunks[k].clear_block_entities()

	var scene := preload("res://level/chunk/chunk.tscn")
	var parent := $Chunks
	var mm_cont := $"MultiMesh Controller"
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
				node.inst = wasm_instance
				node.mm_cont = mm_cont
				parent.add_child(node)
				chunks[coord] = node

	block_entities.clear()

	update_chunks()

func update_chunks() -> void:
	mutex.lock()
	wasm_instance.call_wasm(&"entity_update", [])

	for k in chunks:
		chunks[k].update_chunk()

	mutex.unlock()
	chunks_updated.emit()

func init_empty() -> void:
	mutex.lock()
	wasm_instance.call_wasm(&"init", [chunk_size_x, chunk_size_y, chunk_size_z])
	mutex.unlock()
	init_chunks()

func import_level(data: PackedByteArray) -> void:
	mutex.lock()
	buffer_data = data
	wasm_instance.call_wasm(&"import", [])
	mutex.unlock()
	init_chunks()

func tick() -> void:
	var start := Time.get_ticks_usec()

	mutex.lock()
	wasm_instance.call_wasm(&"tick", [])

	buffer_data = PackedByteArray()
	wasm_instance.call_wasm(&"export_censored", [])
	update_chunks.call_deferred()

	# Gather commands
	var drones := []
	for k in block_entities:
		var v: Dictionary = block_entities[k]
		if v["type"] != "drone":
			continue
		drones.push_back(v["node"])
	for n in drones:
		n.tick(buffer_data)
	for n in drones:
		var uuid: Vector4i = n.uuid
		buffer_data = n.get_command()
		if len(buffer_data) > 0:
			wasm_instance.call_wasm(&"set_command", [uuid.x, uuid.y, uuid.z, uuid.w])
	buffer_data = PackedByteArray()
	mutex.unlock()

	var end := Time.get_ticks_usec()
	#print("Tick: %.3f" % ((end - start) / 1000.0))

func __wasm_random(p: int, n: int) -> void:
	wasm_instance.memory_write(p, crypto.generate_random_bytes(n))

func __wasm_read_buffer(p: int, n: int) -> int:
	if len(buffer_data) > n:
		wasm_instance.signal_error("Buffer is insufficient")
		return 0
	wasm_instance.memory_write(p, buffer_data)
	var ret := len(buffer_data)
	buffer_data = PackedByteArray()
	return ret

func __wasm_write_buffer(p: int, n: int) -> void:
	buffer_data = wasm_instance.memory_read(p, n)

func __wasm_log(p: int, n: int) -> void:
	print(wasm_instance.memory_read(p, n).get_string_from_utf8())

func __wasm_entity_removed(a0: int, a1: int, a2: int, a3: int) -> void:
	var uuid := Vector4i(a0, a1, a2, a3)

	var data = block_entities[uuid]
	block_entities.erase(uuid)

	if data != null:
		if data["type"] == "drone":
			data["node"].queue_free()
		else:
			chunks[data["coord"] / LevelChunk.CHUNK_SIZE].unregister_block_entity(uuid)

func __wasm_entity_iron_ore(a0: int, a1: int, a2: int, a3: int, x: int, y: int, z: int, qty: int) -> void:
	var uuid := Vector4i(a0, a1, a2, a3)
	var data := {
		type = "iron_ore",
		coord = Vector3i(x, y, z),
		quantity = qty,
	}
	data.make_read_only()

	var old = block_entities.get(uuid)
	block_entities[uuid] = data

	if old != null:
		chunks[old["coord"] / LevelChunk.CHUNK_SIZE].unregister_block_entity(uuid)
	chunks[data["coord"] / LevelChunk.CHUNK_SIZE].register_block_entity(uuid, data)

func __wasm_entity_drone(a0: int, a1: int, a2: int, a3: int, x: int, y: int, z: int) -> void:
	var uuid := Vector4i(a0, a1, a2, a3)
	var old = block_entities.get(uuid)

	var node: Node3D
	if old == null:
		node = preload("res://drone/drone.tscn").instantiate()
		node.uuid = uuid
		node.name = "Drone_%8x%8x%8x%8x" % [
			a0 & 0xffff_ffff,
			a1 & 0xffff_ffff,
			a2 & 0xffff_ffff,
			a3 & 0xffff_ffff,
		]
		$Drones.add_child(node)
	else:
		node = old["node"]

	var data := {
		type = "drone",
		coord = Vector3i(x, y, z),
		node = node,
	}
	data.make_read_only()
	block_entities[uuid] = data
	node.update_data(data)
