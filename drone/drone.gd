extends Node3D

@export var uuid := Vector4i.ZERO

var wasm_instance: WasmInstance
var buffer_data := PackedByteArray()
var mutex := Mutex.new()

var channel_ids := {}
var channels: Array[Dictionary] = []

func initialize_wasm(module: WasmModule, config: Dictionary) -> void:
	mutex.lock()
	wasm_instance = WasmInstance.new().initialize(
		module,
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
				"create_channel": {
					params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
					results = [WasmHelper.TYPE_I32],
					callable = __wasm_create_channel,
				},
				"publish_message": {
					params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
					results = [],
					callable = __wasm_publish_message,
				},
				"has_message": {
					params = [WasmHelper.TYPE_I32],
					results = [WasmHelper.TYPE_I32],
					callable = __wasm_has_message,
				},
				"pop_message": {
					params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
					results = [WasmHelper.TYPE_I32],
					callable = __wasm_pop_message,
				},
			},
		},
		config,
	)
	wasm_instance.call_wasm(&"init", [uuid.x, uuid.y, uuid.z, uuid.w])
	mutex.unlock()

func deinitialize_wasm() -> void:
	mutex.lock()
	wasm_instance = null
	mutex.unlock()

func is_wasm_initialized() -> bool:
	return wasm_instance != null

func submit_command(cmd: PackedByteArray) -> void:
	mutex.lock()
	buffer_data = cmd
	mutex.unlock()

func update_data(data: Dictionary) -> void:
	position = Vector3(data["coord"])

func tick(level_data: PackedByteArray) -> PackedByteArray:
	for v in channels:
		v[&"send"].fill(null)
		v[&"send_len"] = 0

	mutex.lock()
	if wasm_instance != null:
		buffer_data = level_data
		wasm_instance.call_wasm(&"tick", [])

	var ret := buffer_data
	buffer_data = PackedByteArray()
	mutex.unlock()
	return ret

func _ready() -> void:
	var c := hash(uuid)
	c ^= c >> 32
	$Mesh.material_override.albedo_color = Color(
		float((c ^ (c >> 24)) & 255) / 255.,
		float(((c >> 8) ^ (c >> 24)) & 255) / 255.,
		float(((c >> 16) ^ (c >> 24)) & 255) / 255.,
	)

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

func __wasm_create_channel(p: int, n: int, flag: int) -> int:
	var chan_name := wasm_instance.memory_read(p, n)
	var i = channel_ids.get(chan_name)
	var data: Dictionary
	if i == null:
		i = len(channels)
		var recv := []
		var send := []
		recv.resize(64)
		send.resize(64)
		data = {
			&"name": chan_name,
			&"flag": flag,
			&"recv": recv,
			&"recv_len": 0,
			&"send": send,
			&"send_len": 0,
		}
		channels.push_back(data)
		channel_ids[chan_name] = i
	else:
		data = channels[i]
		data[&"flag"] = data[&"flag"] | flag

	return i

func __wasm_publish_message(i: int, p: int, n: int) -> void:
	if i >= len(channels):
		wasm_instance.signal_error("Channel index out of bounds")
		return
	var data := channels[i]
	if data[&"flag"] & 1 == 0:
		push_warning("Channel %d is not publishable!" % i)
		return

	var send: Array = data[&"send"]
	var l: int = data[&"send_len"]
	if l < len(send):
		send[l] = wasm_instance.memory_read(p, n)
		data[&"send_len"] = l + 1

func __wasm_has_message(i: int) -> int:
	if i >= len(channels):
		wasm_instance.signal_error("Channel index out of bounds")
		return 0
	return 1 if channels[i][&"recv_len"] > 0 else 0

func __wasm_pop_message(i: int, p: int, n: int) -> int:
	if i >= len(channels):
		wasm_instance.signal_error("Channel index out of bounds")
		return 0
	var data := channels[i]

	var recv: Array = data[&"recv"]
	var l: int = data[&"recv_len"]
	if l == 0:
		return 0
	var msg: PackedByteArray = recv[l - 1]
	if n >= len(msg):
		wasm_instance.memory_write(p, msg)
		data[&"recv_len"] = l - 1
		recv[l] = null
	return len(msg)
