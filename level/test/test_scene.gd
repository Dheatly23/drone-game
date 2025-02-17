extends Node3D

@export var level_gen: WasmModule
@export_range(0.01, 1000.0, 0.01) var tick_delay := 1.0

var thread := Thread.new()
var sema := Semaphore.new()
var mutex := Mutex.new()
var work_msg := "none"

var time_acc := 0.0
var ticking := false

var wasi_ctx := WasiContext.new().initialize({})
var wasm_instance: WasmInstance
var buffer_data := PackedByteArray()
var crypto := Crypto.new()

func thread_fn(level) -> void:
	if level_gen != null:
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

	else:
		level.init_empty()

	while true:
		sema.wait()
		mutex.lock()
		var msg := work_msg
		work_msg = ""
		mutex.unlock()

		match msg:
			"tick":
				level.tick()
			"quit":
				print("Worker thread done!")
				return

func _ready() -> void:
	thread.start(thread_fn.bind($Level))

func _process(delta: float) -> void:
	time_acc += delta
	if time_acc >= tick_delay and not ticking:
		ticking = true
		__send_msg("tick")
		time_acc = 0.0

func _exit_tree() -> void:
	if thread.is_started():
		__send_msg("quit")
		thread.wait_to_finish()

func __send_msg(msg: String) -> void:
	mutex.lock()
	work_msg = msg
	mutex.unlock()
	sema.post()
	#print("Send: %s" % msg)

func __chunks_updated() -> void:
	ticking = false

func __level_inited() -> void:
	# Load WASM module to drone
	var be: Dictionary = $Level.block_entities
	for k in be:
		var v: Dictionary = be[k]
		if v["type"] != "drone":
			continue

		v["node"].initialize_wasm(
			preload("res://wasm/drone_test_simple.wasm"),
			{
				"epoch.enable": true,
				"epoch.timeout": 30.0,
				"wasi.enable": true,
				"wasi.context": wasi_ctx,
				"wasi.args": ["drone"],
			},
		)

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
