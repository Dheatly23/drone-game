# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

extends Node3D
class_name LevelController

const MESH_SIZE := 44
const DRONE_SIZE := 36

@export_range(1, 128) var size_x: int = 1
@export_range(1, 128) var size_y: int = 1
@export_range(1, 128) var size_z: int = 1

var inst: WasmInstance = null
var ptr: int = 0
var meshes: Array[Dictionary] = []

var __written := false
var __key := PackedByteArray()
var __msg := PackedByteArray()

@onready var drones: Array[Node] = $Drones.get_children()

func step():
	if inst == null:
		return

	var data := inst.memory_read(ptr + 12, size_x * size_y * size_z * 4)
	var drone_ptr := inst.get_32(ptr + 28)
	for i in range(len(drones)):
		var d := drones[i]
		var p := drone_ptr + DRONE_SIZE * i
		d.update_data(data, inst.memory_read(p, DRONE_SIZE))
		var c = d.step()
		inst.memory_write(p + 12, c)
	inst.call_wasm(&"step", [])
	update_meshes()

func mark_all_dirty():
	if inst == null:
		return
	inst.call_wasm(&"mark_all_dirty", [])

func pubsub_publish(key: PackedByteArray, msg: PackedByteArray):
	__key = key
	__msg = msg
	inst.call_wasm(&"pubsub_publish", [len(key), len(msg)])
	__key = PackedByteArray()
	__msg = PackedByteArray()

func pubsub_listen(i: int, key: PackedByteArray):
	__key = key
	inst.call_wasm(&"pubsub_listen", [i, len(key)])
	__key = PackedByteArray()

func pubsub_get(i: int):
	__written = false
	inst.call_wasm(&"pubsub_pop", [i])
	return [__key, __msg] if __written else null

func update_meshes():
	if inst == null:
		return

	inst.call_wasm(&"generate_mesh", [])
	var arr := []
	arr.resize(Mesh.ARRAY_MAX)
	for m in meshes:
		var mesh: ArrayMesh = m.mesh
		var p: int = m.p
		if inst.get_8(p) == 0:
			continue

		mesh.clear_surfaces()
		var vertext_cnt := inst.get_32(p + 4)
		arr[Mesh.ARRAY_VERTEX] = inst.get_array(
			inst.get_32(p + 12),
			vertext_cnt,
			TYPE_PACKED_VECTOR3_ARRAY,
		)
		arr[Mesh.ARRAY_NORMAL] = inst.get_array(
			inst.get_32(p + 16),
			vertext_cnt,
			TYPE_PACKED_VECTOR3_ARRAY,
		)
		arr[Mesh.ARRAY_TANGENT] = inst.get_array(
			inst.get_32(p + 20),
			vertext_cnt * 4,
			TYPE_PACKED_FLOAT32_ARRAY,
		)
		arr[Mesh.ARRAY_TEX_UV] = inst.get_array(
			inst.get_32(p + 24),
			vertext_cnt,
			TYPE_PACKED_VECTOR2_ARRAY,
		)
		arr[Mesh.ARRAY_INDEX] = inst.get_array(
			inst.get_32(p + 28),
			inst.get_32(p + 8),
			TYPE_PACKED_INT32_ARRAY,
		)
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arr)

func __read_key(p: int) -> void:
	inst.memory_write(p, __key)

func __read_key_msg(pk: int, pm: int) -> void:
	inst.memory_write(pk, __key)
	inst.memory_write(pm, __msg)

func __write_key_msg(lk: int, pk: int, lm: int, pm: int) -> void:
	__key = inst.memory_read(pk, lk)
	__msg = inst.memory_read(pm, lm)
	__written = true

func _ready():
	var file: WasmFile = load("res://wasm/level_controller.wasm")
	inst = file.instantiate(
		{
			read_key = {
				params = [WasmHelper.TYPE_I32],
				results = [],
				callable = __read_key,
			},
			read_key_msg = {
				params = [WasmHelper.TYPE_I32, WasmHelper.TYPE_I32],
				results = [],
				callable = __read_key_msg,
			},
			write_key_msg = {
				params = [
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
					WasmHelper.TYPE_I32,
				],
				results = [],
				callable = __write_key_msg,
			},
		},
		{
			"engine.use_epoch": true,
			"engine.epoch_autoreset": false,
			"engine.epoch_timeout": 1.0,
		},
	)
	if inst == null:
		printerr("Cannot instantiate level!")
		return

	var ret := inst.call_wasm(&"init", [
		randi() | (randi() << 32),
		size_x,
		size_y,
		size_z,
		len(drones),
	])
	ptr = ret[0]

	var mesh_len := inst.get_32(ptr + 16)
	var mesh_ptr := inst.get_32(ptr + 20)
	var node_ := $Meshes
	for i in range(mesh_len):
		var p := mesh_ptr + MESH_SIZE * i
		var mesh := ArrayMesh.new()
		var node := MeshInstance3D.new()
		node.mesh = mesh
		var x := inst.get_32(p)
		var y := inst.get_32(p + 4)
		var z := inst.get_32(p + 8)
		node.position = Vector3(x, y, z)
		node.name = "Mesh %d,%d,%d" % [x, y, z]
		node_.add_child(node)
		meshes.push_back({
			ptr = p + 12,
			mesh = mesh,
		})

	var drone_ptr := inst.get_32(ptr + 28)
	for i in range(len(drones)):
		var d := drones[i]
		var p := drone_ptr + DRONE_SIZE * i
		inst.put_32(p, d.coord.x)
		inst.put_32(p + 4, d.coord.y)
		inst.put_32(p + 8, d.coord.z)
	inst.call_wasm(&"update_all_drones", [])

	var data := inst.memory_read(ptr + 12, size_x * size_y * size_z * 4)
	for i in range(len(drones)):
		var d := drones[i]
		var p := drone_ptr + DRONE_SIZE * i
		d.update_data(data, inst.memory_read(p, DRONE_SIZE))
