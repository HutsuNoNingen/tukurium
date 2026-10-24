extends CodeEdit
## Accept field references from inspector chips and the Ctrl+RMB carry gesture.
var manager
var field_dropped: Callable

func _can_drop_data(_at: Vector2, data: Variant) -> bool:
	return editable and data is Dictionary and data.get("kind") == "kokoverse_reference" and data.get("reference", {}).get("kind") == "field"

func _drop_data(at: Vector2, data: Variant) -> void:
	if not _can_drop_data(at, data): return
	var point: Vector2i = get_line_column_at_pos(at)
	set_caret_line(point.x)
	set_caret_column(point.y)
	if field_dropped.is_valid(): field_dropped.call(data.reference)

func _gui_input(event: InputEvent) -> void:
	if manager == null or not event is InputEventMouseButton or not event.pressed: return
	if event.button_index == MOUSE_BUTTON_LEFT and not manager.carried_reference.is_empty() and _can_drop_data(event.position, manager.carried_reference):
		if field_dropped.is_valid(): field_dropped.call(manager.carried_reference.reference)
		manager.carried_reference = {}
		accept_event()
