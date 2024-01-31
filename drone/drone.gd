extends Node3D

@export var claw_open := false:
	set(v):
		if v != claw_open and animation != null:
			animation[&"parameters/Claw/playback"].travel(
				&"Claw Engaged" if v else &"Claw Disengaged"
			)
		claw_open = v
@export var coord := Vector3i.ZERO

var level: LevelController = null
var module: WasmModule = null
var inst: WasmInstance = null
var ptr: int = 0

var __key := PackedByteArray()
var __msg := PackedByteArray()

@onready var animation: AnimationTree = $AnimationTree

func update_data(grid_data: PackedByteArray, drone_data: PackedByteArray):
	if inst == null:
		return
	inst.memory_write(inst.get_32(ptr), grid_data)
	inst.memory_write(ptr + 4, drone_data)

func step() -> PackedByteArray:
	if inst == null:
		return PackedByteArray([0, 0, 0])
	inst.call_wasm(&"step", [])
	return inst.memory_read(ptr + 16, 3)

func __read_key_msg(pk: int, pm: int) -> void:
	inst.memory_write(pk, __key)
	inst.memory_write(pm, __msg)

func __pubsub_publish(lk: int, pk: int, lm: int, pm: int) -> void:
	level.pubsub_publish(
		inst.memory_read(pk, lk),
		inst.memory_read(pm, lm),
	)

func __pubsub_listen(lk: int, pk: int) -> void:
	level.pubsub_listen(get_index(), inst.memory_read(lk, pk))

func __pubsub_get() -> void:
	var ret = level.pubsub_get(get_index())
	if ret != null:
		__key = ret[0]
		__msg = ret[1]
		inst.call_wasm(&"read_msg", [len(__key), len(__msg)])
		__key = PackedByteArray()
		__msg = PackedByteArray()

func _ready():
	animation[&"parameters/Claw/playback"].travel(
		&"Claw Engaged" if claw_open else &"Claw Disengaged"
	)

	if level == null or module == null:
		return
	inst = WasmInstance.new().initialize(
		module,
		{
			read_key_msg = {
				params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
				results = [],
				callable = __read_key_msg,
			},
			pubsub_publish = {
				params = [
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
				],
				results = [],
				callable = __pubsub_publish,
			},
			pubsub_listen = {
				params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
				results = [],
				callable = __pubsub_listen,
			},
			pubsub_get = {
				params = [],
				results = [],
				callable = __pubsub_get,
			},
		},
		{
			"engine.use_epoch": true,
			"engine.epoch_autoreset": false,
			"engine.epoch_timeout": 1.0,
		},
	)
	if inst == null:
		return

	var ret := inst.call_wasm(&"init", [
		level.size_x,
		level.size_y,
		level.size_z,
	])
	ptr = ret[0]
