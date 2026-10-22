extends RefCounted

var app
var window: Window
var object_title: Label
var scripts: OptionButton
var templates: OptionButton
var code: CodeEdit
var parameters: VBoxContainer
var enabled: CheckBox
var feedback: Label
var apply_button: Button
var reload_button: Button
var attach_button: Button
var remove_button: Button
var click_button: Button
var parameter_controls: Dictionary = {}
var target := ""
var selected := ""
var pending_id := ""
var pending_action := ""
var filling := false
var source_dirty := false
var base_revision := ""
var config_signature := ""
var list_signature := ""
var queued_parameters: Dictionary = {}
var flush_at := 0
var last_error := ""

func setup(owner) -> void:
	app = owner
	window = Window.new()
	window.title = "Script Systems · Luau"
	window.min_size = Vector2i(720, 560)
	window.size = Vector2i(920, 720)
	window.close_requested.connect(window.hide)
	app.add_child(window)
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for edge in ["left", "top", "right", "bottom"]: margin.add_theme_constant_override("margin_" + edge, 16)
	window.add_child(margin)
	var stack := VBoxContainer.new()
	stack.add_theme_constant_override("separation", 10)
	margin.add_child(stack)
	object_title = Label.new()
	object_title.add_theme_font_size_override("font_size", 20)
	stack.add_child(object_title)
	var row := HBoxContainer.new()
	stack.add_child(row)
	scripts = OptionButton.new()
	scripts.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scripts.item_selected.connect(func(index):
		if source_dirty:
			feedback.text = "Apply the draft or Reload before changing components."
			for i in scripts.item_count:
				if scripts.get_item_metadata(i) == selected: scripts.select(i)
			return
		selected = str(scripts.get_item_metadata(index))
		config_signature = ""
		refresh())
	row.add_child(scripts)
	enabled = CheckBox.new()
	enabled.text = "Run on host"
	enabled.toggled.connect(func(value):
		if not filling: update_component(false, value))
	row.add_child(enabled)
	remove_button = Button.new()
	remove_button.text = "Remove"
	remove_button.pressed.connect(remove_component)
	row.add_child(remove_button)
	var create_row := HBoxContainer.new()
	stack.add_child(create_row)
	templates = OptionButton.new()
	templates.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	create_row.add_child(templates)
	attach_button = Button.new()
	attach_button.text = "+ Add component"
	attach_button.pressed.connect(func(): add_template(templates.selected))
	create_row.add_child(attach_button)
	var description := Label.new()
	description.text = "Edit Luau here. Public fields appear below. Changes run on the host and sync to everyone."
	description.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	stack.add_child(description)
	code = CodeEdit.new()
	code.size_flags_vertical = Control.SIZE_EXPAND_FILL
	code.custom_minimum_size.y = 250
	code.gutters_draw_line_numbers = true
	code.indent_size = 4
	var highlighter := CodeHighlighter.new()
	for keyword in ["return", "function", "end", "local", "for", "in", "do", "if", "then", "else", "elseif", "while", "not", "and", "or", "true", "false", "nil"]:
		highlighter.add_keyword_color(keyword, Color("b6a5ff"))
	highlighter.add_color_region("--", "", Color("7e9b8a"), true)
	highlighter.add_color_region("\"", "\"", Color("d6bb87"))
	highlighter.add_color_region("'", "'", Color("d6bb87"))
	highlighter.number_color = Color("82d9d0")
	highlighter.function_color = Color("c6dce5")
	highlighter.member_variable_color = Color("c6dce5")
	highlighter.symbol_color = Color("9eb5c0")
	code.syntax_highlighter = highlighter
	code.text_changed.connect(func():
		if not filling:
			source_dirty = true
			feedback.text = "Draft · Apply / restart to run this code. Fields reset to their defaults when code changes."
			refresh_buttons())
	stack.add_child(code)
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size.y = 120
	stack.add_child(scroll)
	parameters = VBoxContainer.new()
	parameters.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(parameters)
	feedback = Label.new()
	feedback.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	feedback.custom_minimum_size.y = 48
	stack.add_child(feedback)
	var actions := HBoxContainer.new()
	stack.add_child(actions)
	apply_button = Button.new()
	apply_button.text = "Apply / restart"
	apply_button.pressed.connect(func(): update_component(true, enabled.button_pressed))
	actions.add_child(apply_button)
	reload_button = Button.new()
	reload_button.text = "Reload shared code"
	reload_button.pressed.connect(func():
		source_dirty = false
		queued_parameters.clear()
		config_signature = ""
		last_error = ""
		feedback.text = "Loaded shared component."
		refresh())
	actions.add_child(reload_button)
	click_button = Button.new()
	click_button.text = "Test click"
	click_button.pressed.connect(func(): interact(target))
	actions.add_child(click_button)
	window.hide()

func open(id: String) -> void:
	if target != id:
		target = id
		selected = ""
		source_dirty = false
		config_signature = ""
		list_signature = ""
		queued_parameters.clear()
	refresh()
	window.popup_centered(Vector2i(920, 720))

func current_script() -> Dictionary:
	for component in app.objects.get(target, {}).get("scripts", []):
		if component.id == selected: return component
	return {}

func send_action(action: Dictionary) -> void:
	if not app.editable or not pending_id.is_empty() or not app.objects.has(target): return
	pending_id = "script-%d-%d" % [Time.get_ticks_usec(), randi()]
	pending_action = action.action
	action.command_id = pending_id
	action.target = app.address_of(app.objects[target])
	app.queue_message({"kind":"script", "command":action})
	feedback.text = "Waiting for host…"
	refresh_buttons()

func add_template(index: int) -> void:
	if index < 0 or index >= app.script_templates.size(): return
	send_action({"action":"attach", "source":app.script_templates[index].source})

func remove_component() -> void:
	var component := current_script()
	if component.is_empty(): return
	send_action({"action":"remove", "script_id":component.id, "expected_revision":component.revision})

func update_component(apply_code: bool, run: bool) -> void:
	var component := current_script()
	if component.is_empty(): return
	var source: String = code.text if apply_code else component.source
	if source.to_utf8_buffer().size() > 8192:
		feedback.text = "Code exceeds 8192 bytes."
		return
	var changed_source: bool = apply_code and source != component.source
	var values: Dictionary = {} if changed_source else component.parameters.duplicate(true)
	values.merge(queued_parameters, true)
	queued_parameters.clear()
	send_action({"action":"update", "script_id":component.id,
		"expected_revision":base_revision if changed_source else component.revision,
		"source":source, "enabled":run, "parameters":values})

func queue_parameter(id: String, value: Variant) -> void:
	if filling or source_dirty: return
	queued_parameters[id] = value
	flush_at = Time.get_ticks_msec() + 180

func tick() -> void:
	if not queued_parameters.is_empty() and pending_id.is_empty() and Time.get_ticks_msec() >= flush_at:
		update_component(false, enabled.button_pressed)

func disconnected() -> void:
	if not pending_id.is_empty(): feedback.text = "Connection interrupted. Review the shared component before retrying."
	pending_id = ""
	queued_parameters.clear()
	refresh_buttons()

func receive_result(body: Dictionary) -> bool:
	if pending_id.is_empty() or body.get("command_id", "") != pending_id: return false
	pending_id = ""
	if body.ok:
		if pending_action == "attach":
			selected = ""
			list_signature = ""
		source_dirty = false
		config_signature = ""
		last_error = ""
		feedback.text = "Shared component updated."
	else:
		feedback.text = body.get("message", "Script edit rejected.")
	pending_action = ""
	refresh()
	return true

func refresh() -> void:
	if window == null: return
	var object: Dictionary = app.objects.get(target, {})
	object_title.text = "No object selected"
	if not object.is_empty():
		for field in object.fields:
			if field.id == "name": object_title.text = str(field.value)
	var components: Array = object.get("scripts", [])
	var signature := ""
	for component in components: signature += component.id + component.schema.name
	filling = true
	if signature != list_signature:
		list_signature = signature
		scripts.clear()
		for component in components:
			scripts.add_item(component.schema.name)
			scripts.set_item_metadata(scripts.item_count - 1, component.id)
		if not components.any(func(c): return c.id == selected):
			selected = str(components.back().id) if not components.is_empty() else ""
			config_signature = ""
		for i in scripts.item_count:
			if scripts.get_item_metadata(i) == selected: scripts.select(i)
	if templates.item_count != app.script_templates.size():
		templates.clear()
		for template in app.script_templates: templates.add_item(template.name)
	var component := current_script()
	if component.is_empty():
		source_dirty = false
		code.text = "Add a component template to begin."
		clear_parameters()
	elif config_signature != component.id + component.revision:
		if not source_dirty:
			config_signature = component.id + component.revision
			base_revision = component.revision
			code.text = component.source
			enabled.button_pressed = component.enabled
			build_parameters(component)
		elif component.revision != base_revision:
			feedback.text = "Shared code changed while this draft was open. Reload before retrying."
	for status in app.script_statuses:
		if status.script_id == selected and status.get("error") is String and status.error != last_error:
			last_error = status.error
			feedback.text = "Script stopped: " + status.error
	filling = false
	refresh_buttons()

func clear_parameters() -> void:
	parameter_controls.clear()
	for child in parameters.get_children():
		parameters.remove_child(child)
		child.queue_free()

func build_parameters(component: Dictionary) -> void:
	clear_parameters()
	for field in component.schema.fields:
		var row := HBoxContainer.new()
		parameters.add_child(row)
		var title := Label.new()
		title.text = field.label
		title.custom_minimum_size.x = 160
		row.add_child(title)
		var value: Variant = component.parameters[field.id]
		match field.value_type:
			"number":
				var spin := SpinBox.new()
				spin.min_value = float(field.min) if field.min != null else -1000000000
				spin.max_value = float(field.max) if field.max != null else 1000000000
				spin.step = 0.1
				spin.value = value
				spin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				spin.value_changed.connect(func(v): queue_parameter(field.id, v))
				row.add_child(spin)
				parameter_controls[field.id] = spin
			"bool":
				var check := CheckBox.new()
				check.button_pressed = value
				check.toggled.connect(func(v): queue_parameter(field.id, v))
				row.add_child(check)
				parameter_controls[field.id] = check
			"string":
				var input := LineEdit.new()
				input.text = value
				input.max_length = 512
				input.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				input.text_changed.connect(func(v): queue_parameter(field.id, v))
				row.add_child(input)
				parameter_controls[field.id] = input
			"color":
				var picker := ColorPickerButton.new()
				picker.color = Color(value[0], value[1], value[2], value[3])
				picker.custom_minimum_size.x = 160
				picker.color_changed.connect(func(c): queue_parameter(field.id, [c.r, c.g, c.b, c.a]))
				row.add_child(picker)
				parameter_controls[field.id] = picker
			"vec3":
				var values: Array[SpinBox] = []
				for i in range(3):
					var spin := SpinBox.new()
					spin.min_value = -1000000000
					spin.max_value = 1000000000
					spin.step = 0.01
					spin.value = value[i]
					values.append(spin)
					spin.value_changed.connect(func(_v): queue_parameter(field.id, [values[0].value, values[1].value, values[2].value]))
					row.add_child(spin)
				parameter_controls[field.id] = values

func refresh_buttons() -> void:
	var ready: bool = app.editable and pending_id.is_empty() and app.objects.has(target)
	var component := current_script()
	attach_button.disabled = not ready or source_dirty
	apply_button.disabled = not ready or component.is_empty()
	remove_button.disabled = not ready or component.is_empty() or source_dirty
	enabled.disabled = not ready or component.is_empty() or source_dirty
	code.editable = ready and not component.is_empty()
	parameters.mouse_filter = Control.MOUSE_FILTER_STOP if source_dirty else Control.MOUSE_FILTER_PASS
	parameters.modulate.a = 0.5 if source_dirty else 1.0
	for control in parameter_controls.values():
		if control is SpinBox or control is LineEdit: control.editable = ready and not source_dirty
		elif control is BaseButton: control.disabled = not ready or source_dirty
		elif control is Array:
			for spin in control: spin.editable = ready and not source_dirty
	click_button.disabled = not ready or component.is_empty() or not component.get("schema", {}).get("has_interact", false)

func interact(id: String) -> void:
	if not app.editable or not app.objects.has(id): return
	var object: Dictionary = app.objects[id]
	if object.get("collider") == null: return
	if not object.get("scripts", []).any(func(c): return c.enabled and c.schema.has_interact): return
	app.queue_message({"kind":"script", "command":{"command_id":"interact-%d-%d" % [Time.get_ticks_usec(), randi()], "target":app.address_of(object), "action":"interact"}})
