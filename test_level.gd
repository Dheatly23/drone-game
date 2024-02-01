extends Node3D

func _ready():
	var level := preload("res://level/level.tscn").instantiate()
	level.size_x = 16
	level.size_y = 16
	level.size_z = 16
	level.name = "Level"

	var drone := preload("res://drone/drone.tscn").instantiate()
	drone.module = preload("res://wasm/test_drone.wasm").get_module()
	drone.level = level
	drone.name = "Drone 1"
	level.get_node(^"Drones").add_child(drone)

	add_child(level)
	call_deferred(&"__initialize", level)

func __initialize(level: LevelController):
	var p := level.inst.get_32(level.ptr + 12)
	level.inst.put_32(p + 4, 1)
	level.inst.put_32(p + 8, 2)
	level.mark_all_dirty()
