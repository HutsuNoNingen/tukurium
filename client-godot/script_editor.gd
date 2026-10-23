extends RefCounted

var app
const REFERENCE_CHIP = preload("res://reference_chip.gd")
var action_guard: Callable
var reload_guard: Callable
var selection_guard: Callable
var selection_changed: Callable
var draft_changed: Callable
var external_readonly := false
var shared_document_mode := false
var window: Window
var object_title: Label
var component_count: Label
var add_hint: Label
var scripts: ItemList
var templates: OptionButton
var code: CodeEdit
var parameters: VBoxContainer
var selected_title: Label
var run_status: Label
var run_button: Button
var pause_button: Button
var feedback: Label
var apply_button: Button
var reload_button: Button
var attach_button: Button
var new_button: Button
var remove_confirmation: PanelContainer
var remove_message: Label
var remove_confirm_button: Button
var remove_cancel_button: Button
var removal: Dictionary = {}
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

func setup(owner, mount: Control = null) -> void:
	app = owner
	var surface: Node = mount
	if mount == null:
		window = Window.new()
		window.title = "Script Systems · Luau"
		window.min_size = Vector2i(900, 640)
		window.size = Vector2i(1080, 760)
		window.close_requested.connect(window.hide)
		app.add_child(window)
		surface = window
	else:
		mount.theme = Theme.new()
		mount.theme.default_font_size = 16
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for edge in ["left", "top", "right", "bottom"]: margin.add_theme_constant_override("margin_" + edge, 16)
	surface.add_child(margin)
	var stack := VBoxContainer.new()
	stack.add_theme_constant_override("separation", 10)
	margin.add_child(stack)
	object_title = Label.new()
	object_title.add_theme_font_size_override("font_size", 20)
	stack.add_child(object_title)
	var split := HSplitContainer.new()
	split.size_flags_vertical = Control.SIZE_EXPAND_FILL
	split.split_offset = 260 if mount == null else 170
	stack.add_child(split)
	var sidebar := VBoxContainer.new()
	sidebar.custom_minimum_size.x = 240 if mount == null else 160
	sidebar.add_theme_constant_override("separation", 10)
	split.add_child(sidebar)
	component_count = Label.new()
	sidebar.add_child(component_count)
	scripts = ItemList.new()
	scripts.tooltip_text = "Select a script to edit. Other running scripts keep running."
	scripts.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scripts.custom_minimum_size.y = 140
	scripts.add_theme_constant_override("v_separation", 12)
	scripts.item_selected.connect(select_script)
	sidebar.add_child(scripts)
	remove_button = Button.new()
	remove_button.text = "Remove selected"
	remove_button.focus_mode = Control.FOCUS_NONE
	remove_button.pressed.connect(request_removal)
	sidebar.add_child(remove_button)
	# Keep confirmation inside this surface. An exclusive Window under a 3D
	# SubViewport can block the main viewport without receiving its laser input.
	remove_confirmation = PanelContainer.new()
	sidebar.add_child(remove_confirmation)
	var confirmation_body := VBoxContainer.new()
	remove_confirmation.add_child(confirmation_body)
	remove_message = Label.new()
	remove_message.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	confirmation_body.add_child(remove_message)
	remove_cancel_button = Button.new()
	remove_cancel_button.text = "Cancel"
	remove_cancel_button.focus_mode = Control.FOCUS_NONE
	remove_cancel_button.pressed.connect(cancel_removal)
	confirmation_body.add_child(remove_cancel_button)
	remove_confirm_button = Button.new()
	remove_confirm_button.text = "Remove script"
	remove_confirm_button.focus_mode = Control.FOCUS_NONE
	remove_confirm_button.pressed.connect(remove_component)
	confirmation_body.add_child(remove_confirm_button)
	remove_confirmation.hide()
	sidebar.add_child(HSeparator.new())
	var add_heading := Label.new()
	add_heading.text = "Add a script"
	sidebar.add_child(add_heading)
	templates = OptionButton.new()
	templates.tooltip_text = "Choose starter code for a new script."
	templates.item_selected.connect(func(_index): refresh_buttons())
	sidebar.add_child(templates)
	attach_button = Button.new()
	attach_button.text = "+ Add template"
	attach_button.pressed.connect(func(): add_template(templates.selected))
	sidebar.add_child(attach_button)
	new_button = Button.new()
	new_button.text = "+ Blank script"
	new_button.tooltip_text = "Add your own script with a unique name, then paste or write its code."
	new_button.pressed.connect(add_blank)
	sidebar.add_child(new_button)
	add_hint = Label.new()
	add_hint.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	add_hint.add_theme_font_size_override("font_size", 13)
	sidebar.add_child(add_hint)
	var editor := VBoxContainer.new()
	editor.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	editor.custom_minimum_size.x = 570 if mount == null else 0
	editor.add_theme_constant_override("separation", 10)
	split.add_child(editor)
	var toolbar := HBoxContainer.new()
	editor.add_child(toolbar)
	selected_title = Label.new()
	selected_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	selected_title.add_theme_font_size_override("font_size", 20)
	toolbar.add_child(selected_title)
	run_button = Button.new()
	run_button.text = "Run"
	run_button.tooltip_text = "Run shared callbacks and local Drives. Saved state is preserved."
	run_button.pressed.connect(func(): update_component(false, true))
	toolbar.add_child(run_button)
	pause_button = Button.new()
	pause_button.text = "Pause"
	pause_button.tooltip_text = "Pause this script for everyone. Saved state is preserved."
	pause_button.pressed.connect(func(): update_component(false, false))
	toolbar.add_child(pause_button)
	run_status = Label.new()
	editor.add_child(run_status)
	var code_heading := Label.new()
	code_heading.text = "Source code · Apply to save"
	editor.add_child(code_heading)
	code = CodeEdit.new()
	code.size_flags_vertical = Control.SIZE_EXPAND_FILL
	code.custom_minimum_size.y = 160
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
			feedback.text = "Draft · Apply code to save; Run / Pause stays as shown above. Fields reset when code changes."
			refresh_buttons()
			if draft_changed.is_valid(): draft_changed.call())
	editor.add_child(code)
	var parameters_heading := Label.new()
	parameters_heading.text = "Public parameters · changes save automatically"
	editor.add_child(parameters_heading)
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size.y = 80
	editor.add_child(scroll)
	parameters = VBoxContainer.new()
	parameters.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(parameters)
	feedback = Label.new()
	feedback.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	feedback.custom_minimum_size.y = 48
	editor.add_child(feedback)
	var actions := HBoxContainer.new()
	editor.add_child(actions)
	apply_button = Button.new()
	apply_button.text = "Apply code"
	apply_button.pressed.connect(func(): update_component(true, bool(current_script().get("enabled", false))))
	actions.add_child(apply_button)
	reload_button = Button.new()
	reload_button.text = "Reload shared code"
	reload_button.pressed.connect(func():
		if reload_guard.is_valid():
			reload_guard.call()
			return
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
	if window != null: window.hide()

func open(id: String) -> void:
	if target != id:
		cancel_removal()
		target = id
		selected = ""
		source_dirty = false
		config_signature = ""
		list_signature = ""
		queued_parameters.clear()
	refresh()
	if window != null: window.popup_centered(Vector2i(1080, 760))

func select_script(index: int) -> void:
	if index < 0 or index >= scripts.item_count: return
	if selection_guard.is_valid() and not selection_guard.call(): return
	if (source_dirty and not shared_document_mode) or not queued_parameters.is_empty() or not pending_id.is_empty():
		feedback.text = "Apply or Reload the code draft before switching scripts." if source_dirty else "Wait for the parameter changes to save before switching scripts."
		for i in scripts.item_count:
			if scripts.get_item_metadata(i) == selected: scripts.select(i)
		return
	if shared_document_mode: source_dirty = false
	selected = str(scripts.get_item_metadata(index))
	scripts.select(index)
	config_signature = ""
	last_error = ""
	feedback.text = ""
	refresh()
	if selection_changed.is_valid(): selection_changed.call(selected)

func script_error(id: String) -> String:
	for status in app.local_script_statuses + app.script_statuses:
		if status.script_id == id and status.get("error") is String: return status.error
	return ""

func execution_label(component: Dictionary) -> String:
	if not script_error(str(component.id)).is_empty(): return "Error"
	return "Running" if component.enabled else "Paused"

func current_script() -> Dictionary:
	for component in app.objects.get(target, {}).get("scripts", []):
		if component.id == selected: return component
	return {}

func send_action(action: Dictionary) -> void:
	if action_guard.is_valid():
		action_guard.call(action)
		return
	dispatch_action(action)

func dispatch_action(action: Dictionary) -> void:
	if not app.editable or not pending_id.is_empty() or not app.objects.has(target): return
	pending_action = action.action
	action.target = app.address_of(app.objects[target])
	pending_id = app.requests.request({"kind":"script", "command":action}, receive_result)
	feedback.text = "Waiting for host…"
	refresh_buttons()

func add_template(index: int) -> void:
	if index < 0 or index >= app.script_templates.size(): return
	send_action({"action":"attach", "source":app.script_templates[index].source})

func add_blank() -> void:
	var names: Array = app.objects.get(target,{}).get("scripts",[]).map(func(c): return c.schema.name)
	var number := 1
	while names.has("Script%d" % number): number += 1
	var source := "return {\n    name = \"Script%d\",\n    fields = {},\n    drive = function(ctx)\n        -- Derive values with ctx:drive. Use ctx.time or ctx.world_time.\n    end\n}\n" % number
	send_action({"action":"attach", "source":source})

func remove_component() -> void:
	var component := current_script()
	var requested := removal
	cancel_removal()
	if component.is_empty() or requested.get("target") != target or requested.get("script_id") != component.id or requested.get("revision") != component.revision:
		feedback.text = "The selected script changed. Choose Remove again."
		return
	send_action({"action":"remove", "script_id":component.id, "expected_revision":component.revision})

func request_removal() -> void:
	var component := current_script()
	if component.is_empty() or remove_button.disabled: return
	removal = {"target":target,"script_id":component.id,"revision":component.revision}
	remove_message.text = "Remove %s? Its code, parameters and saved state will be deleted." % component.schema.name
	remove_confirmation.show()

func cancel_removal() -> void:
	removal = {}
	if remove_confirmation != null: remove_confirmation.hide()

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
		update_component(false, bool(current_script().get("enabled", false)))

func disconnected() -> void:
	cancel_removal()
	if not pending_id.is_empty(): feedback.text = "Connection interrupted. Review the shared component before retrying."
	pending_id = ""
	queued_parameters.clear()
	refresh_buttons()

func receive_result(body: Dictionary) -> bool:
	var action := pending_action
	pending_id = ""
	if body.ok:
		if pending_action == "attach":
			selected = ""
			list_signature = ""
		if not shared_document_mode or pending_action in ["attach", "remove"]:
			source_dirty = false
			config_signature = ""
		last_error = ""
		feedback.text = "Shared component updated."
	else:
		feedback.text = body.get("message", "Script edit rejected.")
	pending_action = ""
	refresh()
	if body.ok and shared_document_mode and action in ["attach", "remove"] and selection_changed.is_valid(): selection_changed.call(selected)
	return true

func refresh() -> void:
	if object_title == null: return
	var object: Dictionary = app.objects.get(target, {})
	object_title.text = "No object selected"
	if not object.is_empty():
		for field in object.fields:
			if field.id == "name": object_title.text = "Object: " + str(field.value)
	var components: Array = object.get("scripts", [])
	component_count.text = "Attached scripts · %d / 4" % components.size()
	var signature := ""
	for component in components: signature += component.id + component.schema.name + str(component.enabled) + script_error(component.id)
	filling = true
	if signature != list_signature:
		list_signature = signature
		scripts.clear()
		if components.is_empty():
			scripts.add_item("No scripts attached yet")
			scripts.set_item_disabled(0, true)
		for component in components:
			scripts.add_item(component.schema.name + "  ·  " + execution_label(component))
			scripts.set_item_tooltip(scripts.item_count - 1, script_error(component.id))
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
	if not removal.is_empty() and (removal.get("target") != target or removal.get("script_id") != component.get("id") or removal.get("revision") != component.get("revision")):
		cancel_removal()
	selected_title.text = str(component.get("schema", {}).get("name", "Select a script"))
	run_status.text = "Add a template or a blank script from the left." if component.is_empty() else execution_label(component) + (" · local Drive" if component.get("schema", {}).get("has_drive", false) else " · shared Write on host")
	run_status.modulate = Color("ed9f93") if not script_error(selected).is_empty() else (Color("82d9d0") if component.get("enabled", false) else Color("d6bb87"))
	if component.is_empty():
		source_dirty = false
		code.text = "Add a component template to begin."
		clear_parameters()
	elif config_signature != component.id + component.revision:
		if not source_dirty:
			config_signature = component.id + component.revision
			base_revision = component.revision
			code.text = component.source
			build_parameters(component)
		elif component.revision != base_revision:
			feedback.text = "Shared code changed while this draft was open. Reload before retrying."
	for status in app.local_script_statuses + app.script_statuses:
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
			"field_ref":
				var reference = REFERENCE_CHIP.new()
				reference.manager = app.world_panels
				reference.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				reference.text = "Drop a field reference" if value == null else str(app.world_panels.field_data(value.target.instance_id,"name").get("value","Deleted Slot")) + " · " + str(value.field_id)
				reference.payload = func(): return null if value == null else {"kind":"kokoverse_reference","reference":value}
				reference.accepts = func(data): return data is Dictionary and data.get("kind") == "kokoverse_reference" and data.get("reference",{}).get("kind") == "field"
				reference.received = func(data): queue_parameter(field.id,data.reference)
				reference.pressed.connect(func():
					if value != null: app.world_panels.open_inspector(value.target.instance_id))
				row.add_child(reference)
				parameter_controls[field.id] = reference
				var clear := Button.new()
				clear.text = "Clear"
				clear.focus_mode = Control.FOCUS_NONE
				clear.pressed.connect(func(): queue_parameter(field.id,null))
				row.add_child(clear)
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
	var ready: bool = app.editable and not external_readonly and pending_id.is_empty() and app.objects.has(target)
	var component := current_script()
	var components: Array = app.objects.get(target,{}).get("scripts",[])
	var full := components.size() >= 4
	var duplicate := false
	if templates.selected >= 0 and templates.selected < app.script_templates.size():
		duplicate = components.any(func(c): return c.schema.name == app.script_templates[templates.selected].name)
	attach_button.disabled = not ready or source_dirty or full or duplicate
	attach_button.tooltip_text = "Limit reached: 4 scripts per object." if full else ("This template is already attached. Select it on the left to edit, or add a Blank script." if duplicate else "Attach the chosen template as a new script on this object.")
	add_hint.text = "Up to 4 scripts per object, each with a different name. Use Blank script to write your own."
	if duplicate: add_hint.text = "This template is already attached. Select it on the left, choose another template, or add a Blank script."
	if full: add_hint.text = "4 / 4 scripts attached. Remove a script before adding another."
	if source_dirty: add_hint.text = "Apply or Reload this code draft before adding another script."
	new_button.disabled = not ready or source_dirty or full
	for i in scripts.item_count: scripts.set_item_disabled(i, not ready or components.is_empty())
	apply_button.disabled = not ready or component.is_empty()
	remove_button.disabled = not ready or component.is_empty() or source_dirty
	var can_control := ready and not component.is_empty() and not source_dirty
	run_button.disabled = not can_control or (component.get("enabled", false) and script_error(selected).is_empty())
	pause_button.disabled = not can_control or not component.get("enabled", false)
	reload_button.disabled = not app.editable or not pending_id.is_empty() or component.is_empty()
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
	app.requests.request({"kind":"script", "command":{"target":app.address_of(object), "action":"interact"}}, Callable())
