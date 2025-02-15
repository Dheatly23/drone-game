extends Node3D

@export var uuid := Vector4i.ZERO

var wasm_instance: WasmInstance
var buffer_data := PackedByteArray()

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
