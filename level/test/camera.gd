extends Camera3D

const LOOK_VELOCITY := tan(deg_to_rad(75. / 2))
const MOVE_VELOCITY := 10.0

@export_node_path("Node3D") var select_box: NodePath
@export_node_path("Level") var level: NodePath
@export_node_path("Label") var select_label: NodePath
@export_range(-180, 180, 0.1) var look_x: float:
	get():
		return rad_to_deg(rot_x)
	set(v):
		rot_x = deg_to_rad(v)
@export_range(-90, 90, 0.1) var look_y: float:
	get():
		return rad_to_deg(rot_y)
	set(v):
		rot_y = deg_to_rad(v)

@onready var __select: Node3D = get_node(select_box)
@onready var __level: Node3D = get_node(level)

var selected: Dictionary = {
	found = false,
}

var captured := true
var rot_x := 0.0
var rot_y := 0.0

func _ready() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED

	__select.visible = false

func _process(delta: float) -> void:
	var qx := Quaternion(Vector3.DOWN, rot_x)
	var qy := Quaternion(Vector3.LEFT, rot_y)
	quaternion = qx * qy

	if captured:
		position += (qx * (
			Vector3.FORWARD * (Input.get_action_strength("move_forward") - Input.get_action_strength("move_back")) +
			Vector3.LEFT * (Input.get_action_strength("move_left") - Input.get_action_strength("move_right"))
		) +
		Vector3.UP * (Input.get_action_strength("move_up") - Input.get_action_strength("move_down"))) * (MOVE_VELOCITY * delta)

	var start := Time.get_ticks_usec()
	selected = __level.level_query.query_ray(
		position,
		basis * project_local_ray_normal(get_viewport().get_mouse_position()),
	)
	var end := Time.get_ticks_usec()
	var text: String
	if selected["found"]:
		__select.visible = true
		var pos: Vector3i = selected["pos"]
		__select.position = Vector3(pos) + Vector3.ONE * 0.5
		text = "%d %d %d" % [pos.x, pos.y, pos.z]
		var uuid = selected.get("uuid")
		if uuid != null:
			text = "%s (%08x%08x%08x%08x)" % [text, uuid.x & 0xffff_ffff, uuid.y & 0xffff_ffff, uuid.z & 0xffff_ffff, uuid.w & 0xffff_ffff]
	else:
		__select.visible = false
		text = "###"
	get_node(select_label).text = text + " (time: %.3f)" % ((end - start) * 1e-3)

func _input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		var dv: Vector2 = event.relative * (LOOK_VELOCITY / get_viewport().get_visible_rect().size.y)
		rot_x = wrapf(rot_x + dv.x, -PI, PI)
		rot_y = clampf(rot_y + dv.y, -PI / 2, PI / 2)

func _unhandled_input(event: InputEvent) -> void:
	if event.is_action("release_capture") and event.is_released():
		captured = not captured
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if captured else Input.MOUSE_MODE_VISIBLE
