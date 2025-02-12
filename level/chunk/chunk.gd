extends Node3D

const CHUNK_SIZE := 16

var coord_x: int
var coord_y: int
var coord_z: int

@onready var blocks: ArrayMesh = ArrayMesh.new()

func _ready() -> void:
	$Blocks.mesh = blocks

	position = Vector3(coord_x, coord_y, coord_z) * CHUNK_SIZE

func update_chunk(inst: WasmInstance) -> void:
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
