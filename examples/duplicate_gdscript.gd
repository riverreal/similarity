class_name EnemySpawner
extends Node2D
## Example for similarity-gd: `spawn_wave` and `spawn_boss_wave` are near copies.

signal wave_started(index: int)
signal wave_cleared

@export var enemy_scene: PackedScene
@export_range(1, 50) var enemies_per_wave := 5
@onready var spawn_points: Array[Node] = $SpawnPoints.get_children()

var _alive := 0


func spawn_wave(index: int) -> void:
	wave_started.emit(index)
	var count := enemies_per_wave + index * 2
	for i in range(count):
		var point: Node2D = spawn_points[i % spawn_points.size()]
		var enemy := enemy_scene.instantiate()
		enemy.global_position = point.global_position
		enemy.health = 10 + index * 5
		enemy.died.connect(_on_enemy_died)
		add_child(enemy)
		_alive += 1
	print("wave %d: %d enemies" % [index, count])


func spawn_boss_wave(level: int) -> void:
	wave_started.emit(level)
	var total := enemies_per_wave + level * 2
	for j in range(total):
		var marker: Node2D = spawn_points[j % spawn_points.size()]
		var boss := enemy_scene.instantiate()
		boss.global_position = marker.global_position
		boss.health = 10 + level * 5
		boss.died.connect(_on_enemy_died)
		add_child(boss)
		_alive += 1
	print("boss wave %d: %d enemies" % [level, total])


func _on_enemy_died() -> void:
	_alive -= 1
	if _alive <= 0:
		wave_cleared.emit()
