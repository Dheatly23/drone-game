extends Node

@onready var wasm_instance := WasmInstance.new().initialize(
	preload("res://wasm/level_query.wasm"),
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
		},
	},
	{
		"epoch.enable": true,
		"epoch.timeout": 5.0,
	},
)
var level_data := PackedByteArray()
var crypto: Crypto

func update_level(data: PackedByteArray) -> void:
	level_data = data
	wasm_instance.call_wasm(&"update", [])

func query_ray(pos: Vector3, norm: Vector3) -> Dictionary:
	var p: int = wasm_instance.call_wasm(&"query_ray", [
		pos.x, pos.y, pos.z,
		norm.x, norm.y, norm.z,
	])[0] if not level_data.is_empty() else 0
	var ret: Dictionary
	if p == 0:
		ret = {
			found = false,
		}
		ret.make_read_only()
		return ret

	var a: Array = wasm_instance.read_struct("v3iv4i", p)
	ret = {
		found = true,
		pos = a[0],
	}
	var uuid: Vector4i = a[1]
	if uuid != Vector4i.ZERO:
		ret["uuid"] = uuid
	ret.make_read_only()
	return ret

func __wasm_random(p: int, n: int) -> void:
	wasm_instance.memory_write(p, crypto.generate_random_bytes(n))

func __wasm_read_buffer(p: int, n: int) -> int:
	if len(level_data) > n:
		wasm_instance.signal_error("Buffer is insufficient")
		return 0
	wasm_instance.memory_write(p, level_data)
	return len(level_data)

func __wasm_log(p: int, n: int) -> void:
	print(wasm_instance.memory_read(p, n).get_string_from_utf8())
