extends ColorRect

var prev_state: Input.MouseMode

func _ready() -> void:
	__update(false)

func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.is_action_released("pause"):
		var tree := get_tree()
		tree.paused = not tree.paused
		__update(tree.paused)

func __update(state: bool) -> void:
	visible = state
	if state:
		prev_state = Input.mouse_mode
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	else:
		Input.mouse_mode = prev_state

func __resume() -> void:
	get_tree().paused = false
	__update(false)

func __quit() -> void:
	get_tree().quit()
