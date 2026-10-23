extends Button
## A renderer-local drag surface. All accepted writes still go through Rust commands.
var payload: Callable
var accepts: Callable
var received: Callable
var value_payload: Callable
var manager

func is_reference_chip() -> bool: return true

func _gui_input(event: InputEvent) -> void:
	if manager == null or disabled or not event is InputEventMouseButton or not event.pressed: return
	if event.button_index == MOUSE_BUTTON_RIGHT and event.ctrl_pressed:
		if not manager.carried_reference.is_empty(): manager.carried_reference = {}
		else:
			var source := value_payload if event.shift_pressed and value_payload.is_valid() else payload
			if source.is_valid():
				var data: Variant = source.call()
				if data is Dictionary: manager.carried_reference = data
		accept_event()
	elif event.button_index == MOUSE_BUTTON_LEFT and not manager.carried_reference.is_empty() and _can_drop_data(Vector2.ZERO,manager.carried_reference):
		_drop_data(Vector2.ZERO,manager.carried_reference)
		manager.carried_reference = {}
		accept_event()

func _init() -> void:
	focus_mode = Control.FOCUS_NONE
	text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS

func _get_drag_data(_at: Vector2) -> Variant:
	var source := value_payload if Input.is_key_pressed(KEY_SHIFT) and value_payload.is_valid() else payload
	if not source.is_valid(): return null
	var data: Variant = source.call()
	if data == null: return null
	var preview := Label.new()
	preview.text = text
	set_drag_preview(preview)
	return data

func _can_drop_data(_at: Vector2, data: Variant) -> bool:
	return not disabled and accepts.is_valid() and bool(accepts.call(data))

func _drop_data(_at: Vector2, data: Variant) -> void:
	if _can_drop_data(_at,data) and received.is_valid(): received.call(data)
