extends Node

signal camera_locked(path: NodePath)
signal camera_unlocked()

@export_node_path("Level") var level
@export_node_path("LineEdit") var uuid_text
@export_node_path("CheckButton") var camera_lock_button
@export_node_path("OptionButton") var wasm_list
@export_node_path("Node") var args_node
@export_node_path("Button") var exec_button

@onready var lvl: Level = get_node(level)
@onready var uuid_txt: LineEdit = get_node(uuid_text)
@onready var lock_btn: CheckButton = get_node(camera_lock_button)
@onready var wasm_lst: OptionButton = get_node(wasm_list)
@onready var args_lst := get_node(args_node)
@onready var exec_btn: Button = get_node(exec_button)

var sel_uuid := Vector4i.ZERO
var is_exec := false

func select_drone(uuid) -> void:
	var data = lvl.block_entities.get(uuid) if uuid != null else null
	if data == null:
		sel_uuid = Vector4i.ZERO
		uuid_txt.text = ""
		exec_btn.disabled = true
		exec_btn.text = "Execute"
		lock_btn.disabled = true
		return

	sel_uuid = uuid
	uuid_txt.text = "%08x%08x%08x%08x" % [
		sel_uuid.x & 0xffff_ffff,
		sel_uuid.y & 0xffff_ffff,
		sel_uuid.z & 0xffff_ffff,
		sel_uuid.w & 0xffff_ffff,
	]
	exec_btn.disabled = false
	lock_btn.disabled = false
	camera_lock_toggled()
	is_exec = data["node"].is_wasm_initialized()
	if is_exec:
		exec_btn.text = "Stop Executing"
	else:
		exec_btn.text = "Execute"

func add_argument() -> void:
	var node := preload("res://level/test/arg_line.tscn").instantiate()
	args_lst.add_child(node)
	args_lst.move_child(node, -2)

func exec_toggled() -> void:
	var n = lvl.block_entities[sel_uuid]["node"]

	if is_exec:
		lvl.node_stop_execute(sel_uuid)
	elif wasm_lst.selected != -1:
		var args: Array = [wasm_lst.get_item_text(wasm_lst.selected)]
		for i in range(args_lst.get_child_count() - 1):
			args.push_back(args_lst.get_child(i).get_node(^"Arg").text)

		lvl.node_execute_wasm(
			sel_uuid,
			wasm_lst.get_item_text(wasm_lst.selected),
			args,
			{},
		)

	is_exec = n.is_wasm_initialized()
	if is_exec:
		exec_btn.text = "Stop Executing"
	else:
		exec_btn.text = "Execute"

func camera_lock_toggled() -> void:
	if lock_btn.button_pressed:
		lock_btn.text = "Locked"
		camera_locked.emit(lvl.block_entities[sel_uuid]["node"].get_path())
	else:
		lock_btn.text = "Unlocked"
		camera_unlocked.emit()

func _ready() -> void:
	var wasi_ctx := WasiContext.new().initialize({})
	wasi_ctx.stdout_emit.connect(__log)
	wasi_ctx.stderr_emit.connect(__log)
	wasi_ctx.mount_physical_dir(ProjectSettings.globalize_path("res://js"), "/js")
	wasi_ctx.fs_readonly = true
	lvl.wasi_ctx = wasi_ctx

	var a := lvl.wasm_executables.keys()
	a.sort()
	for k in a:
		wasm_lst.add_item(k)
	wasm_lst.selected = -1

func _process(_delta: float) -> void:
	var exists := lvl.block_entities.has(sel_uuid)
	exec_btn.disabled = not exists
	lock_btn.disabled = not exists

func __log(msg: String) -> void:
	print(msg.strip_edges(false, true))
