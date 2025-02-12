extends Camera3D

const LOOK_VELOCITY := tan(deg_to_rad(75. / 2))
const MOVE_VELOCITY := 10.0

var rot_x := 0.0
var rot_y := 0.0

func _process(delta: float) -> void:
	var qx := Quaternion(Vector3.DOWN, rot_x)
	var qy := Quaternion(Vector3.LEFT, rot_y)
	quaternion = qx * qy

	position += (qx * (
		Vector3.FORWARD * (Input.get_action_strength("move_forward") - Input.get_action_strength("move_back")) +
		Vector3.LEFT * (Input.get_action_strength("move_left") - Input.get_action_strength("move_right"))
	) +
	Vector3.UP * (Input.get_action_strength("move_up") - Input.get_action_strength("move_down"))) * (MOVE_VELOCITY * delta)

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		var dv: Vector2 = event.relative * (LOOK_VELOCITY / get_viewport().get_visible_rect().size.y)
		rot_x = wrapf(rot_x + dv.x, -PI, PI)
		rot_y = clampf(rot_y + dv.y, -PI / 2, PI / 2)
