extends Node3D

@export var level_gen: WasmModule
@export_range(0.01, 1000.0, 0.01) var tick_delay := 1.0

var thread: Thread = null

var time_acc := 0.0
var tick_paused := false
var tick_step := false

var wasm_instance: WasmInstance
var buffer_data := PackedByteArray()
var crypto := Crypto.new()

func _ready() -> void:
	var level := %Level

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
	var level: Level = %Level
	if %Camera.is_locked():
		var d = level.block_entities.get(%DroneEdit.sel_uuid)
		if d != null:
			var command = null

			var is_forward := Input.is_action_just_pressed("move_forward")
			var is_back := Input.is_action_just_pressed("move_back")
			var is_left := Input.is_action_just_pressed("move_left")
			var is_right := Input.is_action_just_pressed("move_right")
			var is_up := Input.is_action_just_pressed("move_up")
			var is_down := Input.is_action_just_pressed("move_down")

			if is_up and not is_down:
				command = level.level_query.query_command(&"command_move_up")
			elif is_down and not is_up:
				command = level.level_query.query_command(&"command_move_down")
			elif is_forward and not is_back:
				command = level.level_query.query_command(&"command_move_forward")
			elif is_back and not is_forward:
				command = level.level_query.query_command(&"command_move_back")
			elif is_left and not is_right:
				command = level.level_query.query_command(&"command_move_left")
			elif is_right and not is_left:
				command = level.level_query.query_command(&"command_move_right")

			if command != null:
				d["node"].buffer_data = command

	if not tick_paused or tick_step:
		time_acc += delta
		if time_acc >= tick_delay and (thread == null or not thread.is_alive()) and level.tick():
			time_acc = 0.0
			tick_step = false

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		if event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			var sel: Dictionary = %Camera.selected
			if sel["found"]:
				handle_select(sel)

func _exit_tree() -> void:
	if thread != null:
		thread.wait_to_finish()

func handle_select(selected: Dictionary) -> void:
	var uuid = selected.get("uuid")
	if uuid != null:
		var data = %Level.block_entities.get(uuid)
		if data != null and data["type"] == "drone":
			%DroneEdit.select_drone(uuid)

func __tick_processed(time: float) -> void:
	%TickTxt.text = "Tick provessed: %.3f ms" % (time * 1e3)

func __pause_toggled(paused: bool) -> void:
	tick_paused = paused
	%PauseBtn.text = "Paused" if paused else "Pause"

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
