extends Node3D
class_name LevelChunk

const CHUNK_SIZE := 16

var coord_x: int
var coord_y: int
var coord_z: int

@onready var blocks: ArrayMesh = ArrayMesh.new()

var inst: WasmInstance
var mm_cont: MultiMeshController
var mm_nodes := {}
var block_entities := {}
var _block_entities_dirty := false

func _ready() -> void:
	$Blocks.mesh = blocks

	position = Vector3(coord_x, coord_y, coord_z) * CHUNK_SIZE

func update_chunk() -> void:
	# Render block entities
	if _block_entities_dirty:
		__render_block_entities()

	# Render chunk
	var p: int = inst.call_wasm(&"get_chunk", [coord_x, coord_y, coord_z])[0]

	if inst.get_8(p) == 0:
		# Clean chunk
		return

	var a := []
	a.resize(Mesh.ARRAY_MAX)
	var attr_len := inst.get_32(p + 4)
	var index_len := inst.get_32(p + 24)
	blocks.clear_surfaces()
	if index_len == 0:
		return
	a[Mesh.ARRAY_VERTEX] = inst.get_array(inst.get_32(p + 8), attr_len, TYPE_PACKED_VECTOR3_ARRAY)
	a[Mesh.ARRAY_NORMAL] = inst.get_array(inst.get_32(p + 12), attr_len, TYPE_PACKED_VECTOR3_ARRAY)
	a[Mesh.ARRAY_TANGENT] = inst.get_array(inst.get_32(p + 16), attr_len * 4, TYPE_PACKED_FLOAT32_ARRAY)
	a[Mesh.ARRAY_TEX_UV] = inst.get_array(inst.get_32(p + 20), attr_len, TYPE_PACKED_VECTOR2_ARRAY)
	a[Mesh.ARRAY_INDEX] = inst.get_array(inst.get_32(p + 28), index_len, TYPE_PACKED_INT32_ARRAY)
	blocks.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, a, [], {}, Mesh.ARRAY_FLAG_USE_DYNAMIC_UPDATE)

func clear_block_entities() -> void:
	block_entities.clear()

func register_block_entity(uuid: Vector4i, data: Dictionary) -> void:
	block_entities[uuid] = data
	_block_entities_dirty = true

func unregister_block_entity(uuid: Vector4i) -> void:
	block_entities.erase(uuid)
	_block_entities_dirty = true

func __render_block_entities() -> void:
	_block_entities_dirty = false

	for k: String in mm_nodes:
		for n: MultiMeshInstance3D in mm_nodes[k]:
			n.multimesh.visible_instance_count = 0

	var n := {}
	for uuid: Vector4i in block_entities:
		var data: Dictionary = block_entities[uuid]
		var ty: String = data["type"]
		var i: int = n.get(ty, 0)
		match ty:
			"iron_ore":
				__add_block_entity(
					i,
					ty,
					preload("res://block_entities/iron_ore/model.obj"),
					preload("res://block_entities/iron_ore/material.tres"),
					data["coord"],
				)
			_:
				continue
		n[ty] = i + 1

	for k: String in mm_nodes:
		var v: Array = mm_nodes[k]
		while not v.is_empty():
			var node: MultiMeshInstance3D = v[-1]
			if node.multimesh.visible_instance_count > 0:
				break
			v.pop_back()
			remove_child(node)
			mm_cont.return_multimesh_node(node)

func __add_block_entity(i: int, ty: String, mesh: Mesh, mat: Material, coord: Vector3i) -> void:
	var a = mm_nodes.get(ty)
	if a == null:
		a = []
		mm_nodes[ty] = a

	var j: int = i / MultiMeshController.INSTANCE_COUNT
	var node: MultiMeshInstance3D = a[j] if j < len(a) else null
	if node == null or node.multimesh.visible_instance_count == MultiMeshController.INSTANCE_COUNT or node.multimesh.visible_instance_count == -1:
		node = mm_cont.get_multimesh_node(mesh, mat)
		add_child(node)
		a.append(node)

	var mm := node.multimesh
	j = i % MultiMeshController.INSTANCE_COUNT
	mm.visible_instance_count = j + 1
	mm.set_instance_transform(j, Transform3D(Basis.IDENTITY, Vector3(coord % CHUNK_SIZE)))
