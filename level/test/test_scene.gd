extends Node3D

@export var level_gen: WasmModule

@onready var thread := Thread.new()

var wasm_instance: WasmInstance
var buffer_data := PackedByteArray()
var crypto := Crypto.new()

func _ready() -> void:
	var level := $Level

	if level_gen != null:
		var fn := func ():
			var start := Time.get_ticks_usec()
			wasm_instance = WasmInstance.new().initialize(
				level_gen,
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
					},
				},
				{
					"epoch.enable": true,
					"epoch.timeout": 30.0,
				},
			)

			wasm_instance.call_wasm(&"generate", [])

			var end := Time.get_ticks_usec()
			print("Done generating in %f seconds" % ((end - start) / 1e6))

			level.import_level.call_deferred(buffer_data)
			buffer_data = PackedByteArray()
			wasm_instance = null

		thread.start(fn)

	level.init_empty()

func _exit_tree() -> void:
	if thread.is_started():
		thread.wait_to_finish()

func __wasm_random(p: int, n: int) -> void:
	wasm_instance.memory_write(p, crypto.generate_random_bytes(n))

func __wasm_read_buffer(p: int, n: int) -> int:
	if len(buffer_data) > n :
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
