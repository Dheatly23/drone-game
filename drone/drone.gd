# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

extends Node3D

signal emit_log(message: String)

@export var config_data := PackedByteArray()
@export var claw_open := false:
	set(v):
		if v != claw_open and animation != null:
			animation[&"parameters/Claw/playback"].travel(
				&"Claw Engaged" if v else &"Claw Disengaged"
			)
		claw_open = v
@export var coord := Vector3i.ZERO:
	set(v):
		position = Vector3(v) + Vector3(0.5, 0.5, 0.5)
		coord = v

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

func __log(p: int, n: int) -> void:
	emit_log.emit(inst.memory_read(p, n).get_string_from_utf8())

func __read_key_msg(pk: int, pm: int) -> void:
	inst.memory_write(pk, __key)
	inst.memory_write(pm, __msg)

func __pubsub_publish(pk: int, lk: int, pm: int, lm: int) -> void:
	level.pubsub_publish(
		inst.memory_read(pk, lk),
		inst.memory_read(pm, lm),
	)

func __pubsub_listen(pk: int, lk: int) -> void:
	level.pubsub_listen(get_index(), inst.memory_read(pk, lk))

func __pubsub_get() -> void:
	var ret = level.pubsub_get(get_index())
	if ret != null:
		__key = ret[0]
		__msg = ret[1]
		inst.call_wasm(&"read_msg", [len(__key), len(__msg)])
		__key = PackedByteArray()
		__msg = PackedByteArray()

func __get_config_length() -> int:
	return len(config_data)

func __get_config(p: int) -> void:
	inst.memory_write(p, config_data)

func _ready():
	animation[&"parameters/Claw/playback"].travel(
		&"Claw Engaged" if claw_open else &"Claw Disengaged"
	)

	if level == null or module == null:
		return
	inst = WasmInstance.new().initialize(
		module,
		{
			host = {
				log = {
					params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
					results = [],
					callable = __log,
				},
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
				get_config_length = {
					params = [],
					results = [WasmHelper.TYPE_I32],
					callable = __get_config_length,
				},
				get_config = {
					params = [WasmHelper.TYPE_I32],
					results = [],
					callable = __get_config,
				},
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
