extends Node3D

@export var level_gen: WasmModule
@export_range(0.01, 1000.0, 0.01) var tick_delay := 1.0

var thread: Thread = null

var time_acc := 0.0
var tick_paused := false
var tick_step := false

var wasi_ctx := WasiContext.new().initialize({})
var wasm_instance: WasmInstance
var buffer_data := PackedByteArray()
var crypto := Crypto.new()

func _ready() -> void:
	wasi_ctx.stdout_emit.connect(__log)
	wasi_ctx.stderr_emit.connect(__log)
	wasi_ctx.mount_physical_dir(ProjectSettings.globalize_path("res://js"), "/js")
	wasi_ctx.fs_readonly = true

	var level := $Level

	if level_gen != null:
		var c := func():
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

		thread = Thread.new()
		thread.start(c)

	else:
		level.init_empty()

func _process(delta: float) -> void:
	if not tick_paused or tick_step:
		time_acc += delta
		if time_acc >= tick_delay and (thread == null or not thread.is_alive()) and $Level.tick():
			time_acc = 0.0
			tick_step = false

func _exit_tree() -> void:
	if thread != null:
		thread.wait_to_finish()

func __level_inited() -> void:
	# Load WASM module to drone
	var be: Dictionary = $Level.block_entities
	for k in be:
		var v: Dictionary = be[k]
		if v["type"] != "drone":
			continue

		v["node"].initialize_wasm(
			preload("res://wasm/drone_js.wasm"),
			{
				"epoch.enable": true,
				"epoch.timeout": 30.0,
				"wasi.enable": true,
				"wasi.context": wasi_ctx,
				"wasi.args": ["drone", "/js/simple.js"],
				"wasi.stdout.bindMode": "context",
				"wasi.stdout.bufferMode": "line",
				"wasi.stderr.bindMode": "context",
				"wasi.stderr.bufferMode": "line",
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

func __log(msg: String) -> void:
	print(msg.strip_edges(false, true))

func __tick_processed(time: float) -> void:
	%TickTxt.text = "Tick provessed: %.3f ms" % (time * 1e3)

func __pause_toggled(paused: bool) -> void:
	tick_paused = paused
	%PauseBtn.text = "Paused" if paused else "Pause"
