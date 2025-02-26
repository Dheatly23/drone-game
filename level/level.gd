extends Node3D
class_name Level

signal chunks_updated()
signal initialized()
signal tick_processed(time: float)

@export_group("Chunk Size")
@export_range(1, 64) var chunk_size_x := 1
@export_range(1, 64) var chunk_size_y := 1
@export_range(1, 64) var chunk_size_z := 1

@onready var wasm_instance := WasmInstance.new().initialize(
	preload("res://wasm/level_controller.wasm"),
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
@onready var level_query := $Query

var chunks := {}
var block_entities := {}

var buffer_data := PackedByteArray()
var crypto := Crypto.new()
var mutex := Mutex.new()
var thread: Thread = null

var level_data := PackedByteArray()

var __work_mutex := Mutex.new()
var __sema := Semaphore.new()
var __quitting := false
var __ticking := false

func _ready() -> void:
	$Query.crypto = crypto

func _exit_tree() -> void:
	__shutdown_thread()

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

	update_chunks(true)

func update_chunks(init: bool = false) -> void:
	mutex.lock()
	if init:
		buffer_data = PackedByteArray()
		wasm_instance.call_wasm(&"export_censored", [])
		level_data = buffer_data
		buffer_data = PackedByteArray()
	wasm_instance.call_wasm(&"entity_update", [])

	for k in chunks:
		chunks[k].update_chunk()

	mutex.unlock()
	__work_mutex.lock()
	__ticking = false
	__work_mutex.unlock()

	if init:
		initialized.emit()
	chunks_updated.emit()

func init_empty() -> void:
	__shutdown_thread()

	mutex.lock()
	wasm_instance.call_wasm(&"init", [chunk_size_x, chunk_size_y, chunk_size_z])
	mutex.unlock()
	init_chunks()

func import_level(data: PackedByteArray) -> void:
	__shutdown_thread()

	mutex.lock()
	buffer_data = data
	wasm_instance.call_wasm(&"import", [])
	mutex.unlock()
	init_chunks()

func tick() -> bool:
	__work_mutex.lock()

	if thread == null:
		__quitting = false
		__ticking = false
		thread = Thread.new()
		thread.start(__thread_fn)

	if __ticking:
		__work_mutex.unlock()
		return false

	__ticking = true
	__work_mutex.unlock()
	__sema.post()
	return true

func __thread_fn() -> void:
	while true:
		__sema.wait()
		__work_mutex.lock()
		if __quitting:
			__work_mutex.unlock()
			return
		elif __ticking:
			__work_mutex.unlock()
			__tick_fn()
			continue
		__work_mutex.unlock()

func __tick_fn() -> void:
	var start := Time.get_ticks_usec()

	# Gather commands
	var drones := []
	for k in block_entities:
		var v: Dictionary = block_entities[k]
		if v["type"] != "drone":
			continue
		drones.push_back(v["node"])

	mutex.lock()
	var c := __drone_work.bind(drones, level_data)
	mutex.unlock()
	var group_id := WorkerThreadPool.add_group_task(
		c,
		len(drones),
		-1,
		false,
		"Drone Work",
	)
	WorkerThreadPool.wait_for_group_task_completion(group_id)

	# Tick
	mutex.lock()
	wasm_instance.call_wasm(&"tick", [])
	buffer_data = PackedByteArray()
	wasm_instance.call_wasm(&"export", [])
	var uncen := buffer_data
	buffer_data = PackedByteArray()
	wasm_instance.call_wasm(&"export_censored", [])
	var cen := buffer_data
	__tick_main.call_deferred(uncen, cen)
	buffer_data = PackedByteArray()
	mutex.unlock()

	# Transfer pubsub
	var send := {}
	var recv := {}
	for n in drones:
		var channel_ids: Dictionary = n.channel_ids
		var channels = n.channels
		for k in channel_ids:
			var d: Dictionary = channels[channel_ids[k]]
			var flag: int = d[&"flag"]
			if flag & 1 != 0 and d[&"send_len"] > 0:
				send.get_or_add(k, []).push_back({
					d = d,
					n = 0,
				})
			if flag & 2 != 0 and d[&"recv_len"] < 64:
				recv.get_or_add(k, []).push_back(d)

	for k in send:
		var a: Array = send[k]
		var t := []
		t.resize(64)
		send[k] = t
		for i in range(len(t)):
			if a.is_empty():
				break
			var j := randi_range(0, len(a) - 1)
			var v: Dictionary = a[j]
			var d: Dictionary = v["d"]
			var n: int = v["n"]
			t[i] = d[&"send"][n]
			n += 1
			if n == d[&"send_len"]:
				a.remove_at(j)
			else:
				v["n"] = n

	for k in recv:
		var t = send.get(k)
		if t is not Array:
			continue
		for v in recv[k]:
			var r = v[&"recv"]
			var rl: int = v[&"recv_len"]
			for msg in t:
				if msg == null or rl == len(r):
					break
				r[rl] = msg
				rl += 1
			v[&"recv_len"] = rl

	var end := Time.get_ticks_usec()
	tick_processed.emit.call_deferred((end - start) * 1e-6)

func __drone_work(ix, drones: Array, data: PackedByteArray) -> void:
	var n = drones[ix]
	var uuid: Vector4i = n.uuid
	var buf = n.tick(data)
	mutex.lock()
	buffer_data = buf
	wasm_instance.call_wasm(&"set_command", [uuid.x, uuid.y, uuid.z, uuid.w])
	buffer_data = PackedByteArray()
	mutex.unlock()

func __tick_main(data: PackedByteArray, data_censored: PackedByteArray) -> void:
	level_data = data_censored
	$Query.update_level(data)
	update_chunks()

func __shutdown_thread() -> void:
	if thread != null:
		__work_mutex.lock()
		__quitting = true
		__work_mutex.unlock()
		__sema.post()
		thread.wait_to_finish()
		thread = null

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
