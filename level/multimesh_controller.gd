extends Node3D
class_name MultiMeshController

const INSTANCE_COUNT := 8192

func get_multimesh_node(mesh: Mesh, mat: Material) -> MultiMeshInstance3D:
	var node: MultiMeshInstance3D
	if get_child_count() > 0:
		node = get_child(-1)
		remove_child(node)
	else:
		node = __make_node()

	node.multimesh.mesh = mesh
	node.material_override = mat

	return node

func return_multimesh_node(node: MultiMeshInstance3D) -> void:
	if get_child_count() >= 16:
		node.free()
	else:
		node.multimesh.mesh = null
		node.material_override = null
		__add_node(node)

func _ready() -> void:
	for _i in range(4):
		__add_node(__make_node())

func _process(_delta: float) -> void:
	while get_child_count() < 4:
		__add_node(__make_node())

func __make_node() -> MultiMeshInstance3D:
	var mesh := MultiMesh.new()
	mesh.transform_format = MultiMesh.TRANSFORM_3D
	mesh.instance_count = 8192
	mesh.visible_instance_count = 0

	var node := MultiMeshInstance3D.new()
	node.multimesh = mesh
	return node

func __add_node(node: Node) -> void:
	node.name = "MM_Cached_%d" % get_child_count()
	add_child(node)
