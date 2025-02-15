extends Node3D

@export var uuid := Vector4i.ZERO

var wasm_instance: WasmInstance

func update_data(data: Dictionary) -> void:
	position = Vector3(data["coord"])
