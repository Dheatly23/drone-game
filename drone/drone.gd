extends Node3D

@export var uuid := Vector4i.ZERO

var wasm_instance: WasmInstance
var buffer_data := PackedByteArray()

func initialize_wasm(module: WasmModule, config: Dictionary) -> void:
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
			},
		},
		config,
	)
	wasm_instance.call_wasm(&"init", [uuid.x, uuid.y, uuid.z, uuid.w])

func update_data(data: Dictionary) -> void:
	position = Vector3(data["coord"])

func tick(level_data: PackedByteArray) -> void:
	if wasm_instance != null:
		buffer_data = level_data
		wasm_instance.call_wasm(&"tick", [])

func get_command() -> PackedByteArray:
	var ret := buffer_data
	buffer_data = PackedByteArray()
	return ret

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
