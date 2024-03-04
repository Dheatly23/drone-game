extends Node3D

func __log(message: String):
	# For now, print
	print(message)

func _ready():
	var level := preload("res://level/level.tscn").instantiate()
	level.size_x = 16
	level.size_y = 16
	level.size_z = 16
	level.tick_count = 128
	level.name = "Level"
	level.emit_log.connect(__log)

	var drone := preload("res://drone/drone.tscn").instantiate()
	drone.module = preload("res://wasm/test_drone.wasm")
	drone.level = level
	drone.name = "Drone 1"
	drone.emit_log.connect(__log)
	drone.coord = Vector3i(0, 1, 0)
	level.get_node(^"Drones").add_child(drone)

	add_child(level)
	call_deferred(&"__initialize", level)

func __initialize(level: LevelController):
	var p := level.inst.get_32(level.ptr + 12)
	level.inst.put_32(p + 4, 1)
	level.inst.put_32(p + 8, 2)
	level.mark_all_dirty()

	p = level.inst.get_32(level.ptr + 28)
	for i in range(p + 16, p + 52, 4):
		level.inst.put_32(i, 0x0040_0001)
