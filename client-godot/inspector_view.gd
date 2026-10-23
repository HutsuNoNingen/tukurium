extends PanelContainer
const SHARED_SCRIPT_VIEW = preload("res://shared_script_view.gd")
const INSPECTOR_TREE = preload("res://inspector_tree.gd")
const REFERENCE_CHIP = preload("res://reference_chip.gd")
## One inspector instance. Selection, drafts and requests never use the HUD's singleton state.
var manager
var script_view
var script_mount: MarginContainer
var scripts_button: Button
var panel_id := ""
var target_id := ""
var panel_state: Dictionary = {}
var controls: Dictionary = {}
var edits: Dictionary = {}
var filling := false
var title := Label.new()
var target_picker := OptionButton.new()
var scroll := ScrollContainer.new()
var fields_box := VBoxContainer.new()
var message := Label.new()
var pin_button: Button
var picker_ids: Array[String] = []
var picker_signature := ""
var built := false
var scroll_pending := -1.0
var scroll_inflight := false
var scroll_due := 0
var remote_fields: Dictionary = {}
var target_change_pending := false
var pending_target: Variant = null
var close_when_ready := false
var field_container: VBoxContainer
var drive_labels: Dictionary = {}
var drive_buttons: Dictionary = {}
var field_signature := ""
var requested_script := ""
var hierarchy
var input_client_id := ""
var desktop := false
var building_field: Dictionary = {}
var reference_chips: Dictionary = {}
var pending_field := ""
var pending_reference_target := ""
var reference_values: Dictionary = {}

func setup(controller, id: String, in_desktop: bool = false) -> void:
	manager = controller
	panel_id = id
	desktop = in_desktop
	input_client_id = manager.client_id + "/" + str(get_instance_id())
	theme = Theme.new()
	theme.default_font_size = 17 if desktop else 26
	var style := StyleBoxFlat.new()
	style.bg_color = Color("15212e")
	style.border_color = Color("5ac8c5")
	style.set_border_width_all(0 if desktop else 3)
	style.set_corner_radius_all(0 if desktop else 10)
	add_theme_stylebox_override("panel", style)
	build_layout()
	connect_view()

func build_layout() -> void:
	var margin := MarginContainer.new()
	for side in ["left", "top", "right", "bottom"]: margin.add_theme_constant_override("margin_"+side, 16)
	add_child(margin)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 6 if desktop else 12)
	margin.add_child(column)
	var header := HBoxContainer.new()
	column.add_child(header)
	title.text = "Inspector"
	title.add_theme_font_size_override("font_size", 27)
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(title)
	add_button(header, "+", func(): manager.duplicate_panel(panel_id))
	add_button(header, "↗", func(): manager.open_window(panel_id)).tooltip_text = "Open in a desktop window"
	pin_button = add_button(header, "Pin", func(): manager.pin_panel(panel_id))
	add_button(header, "×", func(): manager.close_panel(panel_id)).tooltip_text = "Delete this shared inspector for everyone"
	column.add_child(target_picker)
	var actions := HBoxContainer.new()
	column.add_child(actions)
	build_actions(actions)
	var body := HBoxContainer.new()
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(body)
	build_body(body)
	column.add_child(message)

func connect_view() -> void:
	target_picker.item_selected.connect(func(index: int):
		if filling or index >= picker_ids.size(): return
		finish_all()
		var id_value: String = picker_ids[index]
		var target: Variant = manager.app.address_of(manager.app.objects[id_value]) if manager.app.objects.has(id_value) else null
		pending_target = target
		target_change_pending = true)
	scroll.get_v_scroll_bar().value_changed.connect(on_scroll)
	message.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	message.add_theme_font_size_override("font_size", 16 if desktop else 18)
	message.text = "Changes are shared with everyone" if desktop else "Shared panel · FPS: RMB hold · Wheel distance · E rotate"

func build_actions(actions: Container) -> void:
	add_button(actions, "Select / Gizmo", func():
		if not target_id.is_empty(): manager.app.select_object(target_id))
	scripts_button = add_button(actions, "Scripts", func():
		if not script_view.busy():
			manager.edit(panel_id,"inspector_scripts_open",not bool(panel_state.inspector.get("scripts_open",false))))
	add_button(actions, "New inspector", func(): manager.open_inspector(target_id))
	var receiver = REFERENCE_CHIP.new()
	receiver.manager = manager
	receiver.text = "Drop reference"
	receiver.custom_minimum_size.x = 155
	receiver.accepts = func(data): return data is Dictionary and data.get("kind") == "kokoverse_reference"
	receiver.received = func(data):
		var reference: Dictionary = data.reference
		pending_field = reference.get("field_id", "")
		pending_reference_target = str(reference.target.instance_id)
		choose_target(str(reference.target.instance_id))
	receiver.tooltip_text = "Drop a Slot, component or field reference to inspect its object"
	actions.add_child(receiver)

func build_body(body: Container) -> void:
	hierarchy = INSPECTOR_TREE.new()
	hierarchy.setup(self)
	body.add_child(hierarchy)
	var details := VBoxContainer.new()
	details.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(details)
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	details.add_child(scroll)
	fields_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	fields_box.add_theme_constant_override("separation", 10)
	scroll.add_child(fields_box)
	script_mount = MarginContainer.new()
	script_mount.size_flags_vertical = Control.SIZE_EXPAND_FILL
	details.add_child(script_mount)
	script_mount.hide()
	script_view = SHARED_SCRIPT_VIEW.new()
	script_view.setup(manager,panel_id,script_mount)

func add_button(parent: Node, text: String, action: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.focus_mode = Control.FOCUS_NONE
	button.pressed.connect(action)
	parent.add_child(button)
	return button

func heading(text: String) -> void:
	var label = REFERENCE_CHIP.new() if not building_field.is_empty() else Label.new()
	label.text = text
	if label is Label: label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	else:
		label.manager = manager
		label.flat = true
		label.alignment = HORIZONTAL_ALIGNMENT_LEFT
		var field := str(building_field.id)
		label.payload = func(): return {"kind":"kokoverse_reference","reference":{"kind":"field","target":manager.app.field_address(target_id,field),"field_id":field}}
		label.value_payload = func():
			var data: Dictionary = manager.field_data(target_id,field)
			return {"kind":"kokoverse_value","value_type":data.value_type,"value":data.value}
		label.accepts = func(data): return data is Dictionary and data.get("kind") == "kokoverse_value" and data.get("value_type") in ["string","vec3","quat","color"] and data.get("value_type") == manager.field_data(target_id,field).get("value_type") and controls.has(field) and manager.field_driver(target_id,field).is_empty()
		label.received = func(data): apply_dropped_value(field,data.value)
		label.tooltip_text = "Drag: field reference · Shift+drag: copy shared value"
		reference_chips[field] = label
	label.add_theme_color_override("font_color", Color("9cc6d4"))
	field_container.add_child(label)

func line_field(parent: Node, field: String, part: String) -> LineEdit:
	var line := LineEdit.new()
	line.custom_minimum_size = Vector2(0, 30 if desktop else 44)
	line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.max_length = 80 if field == "name" else 32
	line.set_meta("field", field)
	line.set_meta("part", part)
	parent.add_child(line)
	if not controls.has(field): controls[field] = []
	controls[field].append(line)
	line.focus_entered.connect(func(): start_edit(field, line))
	line.focus_exited.connect(func():
		if edits.has(field): edits[field].release = true)
	line.text_changed.connect(func(_text): changed(field, line))
	line.text_submitted.connect(func(_text): line.release_focus())
	return line

func vector_field(field: String, label: String, parts: Array) -> void:
	heading(label)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 10)
	field_container.add_child(row)
	for i in range(parts.size()):
		var column := VBoxContainer.new()
		column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		row.add_child(column)
		var axis := Label.new()
		axis.text = parts[i]
		axis.add_theme_font_size_override("font_size", 18)
		column.add_child(axis)
		line_field(column, field, str(i))

func build_fields() -> void:
	for child in fields_box.get_children():
		fields_box.remove_child(child)
		child.queue_free()
	controls.clear()
	drive_labels.clear()
	drive_buttons.clear()
	reference_chips.clear()
	reference_values.clear()
	building_field = {}
	field_container = fields_box
	if target_id.is_empty() or not manager.app.objects.has(target_id):
		heading("Choose an object above. Deleted targets are cleared.")
		return
	var object: Dictionary = manager.app.objects[target_id]
	for schema in manager.component_schemas:
		var fields: Array = object.fields.filter(func(field): return int(field.component_id) == int(schema.local_id))
		if fields.is_empty(): continue
		component_section(str(schema.type_id),int(schema.local_id))
		for field in fields: build_field(field)
	building_field = {}
	component_section("Scripts",8)
	if object.get("scripts", []).is_empty(): heading("No scripts attached")
	for script in object.get("scripts", []):
		var row := HBoxContainer.new()
		field_container.add_child(row)
		var label := Label.new()
		label.text = script.schema.name + (" · Running" if script.enabled else " · Paused")
		label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		label.add_theme_font_size_override("font_size", 21)
		row.add_child(label)
		add_button(row, "Edit", open_script.bind(str(script.id)))
	add_button(field_container, "Manage scripts", open_script.bind(""))

func component_section(name: String, component_id: int = 0) -> void:
	var section := VBoxContainer.new()
	section.add_theme_constant_override("separation", 6)
	fields_box.add_child(section)
	var rule := HSeparator.new()
	section.add_child(rule)
	var label = REFERENCE_CHIP.new()
	label.manager = manager
	label.text = name
	label.flat = true
	label.alignment = HORIZONTAL_ALIGNMENT_LEFT
	label.payload = func(): return {"kind":"kokoverse_reference","reference":{"kind":"component","target":{"instance_id":target_id,"local_id":component_id}}}
	label.tooltip_text = "Drag this component reference to another inspector"
	label.add_theme_font_size_override("font_size", 19 if desktop else 23)
	label.add_theme_color_override("font_color", Color("5ac8c5"))
	section.add_child(label)
	field_container = section

func build_field(field: Dictionary) -> void:
		building_field = field
		match field.value_type:
			"address":
				heading(field.label)
				var reference = REFERENCE_CHIP.new()
				reference.manager = manager
				reference.text = "World" if field.value == null else str(manager.field_data(field.value.instance_id,"name").get("value","Deleted Slot"))
				reference_values[field.id] = reference
				reference.payload = func():
					var value: Variant = manager.field_data(target_id,field.id).value
					return null if value == null else {"kind":"kokoverse_reference","reference":{"kind":"slot","target":value}}
				reference.pressed.connect(func():
					var value: Variant = manager.field_data(target_id,field.id).value
					if value != null: manager.open_inspector(value.instance_id))
				if field.id == "parent":
					reference.accepts = func(data): return data is Dictionary and data.get("kind") == "kokoverse_reference" and data.get("reference",{}).get("kind") == "slot"
					reference.received = func(data): hierarchy.move_object(target_id,data.reference.target.instance_id)
				field_container.add_child(reference)
			"inspector_tree":
				pass
			"string":
				heading(field.label)
				line_field(field_container, field.id, "0")
			"vec3": vector_field(field.id, field.label, ["X", "Y", "Z"])
			"quat": vector_field(field.id, field.label + " · degrees", ["X", "Y", "Z"])
			"color": vector_field(field.id, field.label, ["R", "G", "B", "A"])
			"collider":
				heading("Collider")
				var row := HBoxContainer.new()
				field_container.add_child(row)
				controls[field.id] = []
				for label in ["Enabled / grabbable", "Solid"]:
					var check := CheckBox.new()
					check.text = label
					check.focus_mode = Control.FOCUS_NONE
					row.add_child(check)
					controls[field.id].append(check)
					check.toggled.connect(func(_enabled): changed(field.id, check, true))
				vector_field(field.id, "Collider half extents", ["X", "Y", "Z"])
			"playback":
				if field.value is Dictionary:
					heading("Video · shared playback")
					var row := HBoxContainer.new()
					field_container.add_child(row)
					add_button(row, "Play", func(): playback("play"))
					add_button(row, "Pause", func(): playback("pause"))
					add_button(row, "Restart", func(): playback("restart"))
			_:
				heading(field.label + " · " + JSON.stringify(field.value))
		if field.id in ["position", "rotation", "scale", "color"]:
			var info := Label.new()
			info.add_theme_font_size_override("font_size", 18)
			info.add_theme_color_override("font_color", Color("c5a5ed"))
			info.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
			field_container.add_child(info)
			drive_labels[field.id] = info
			var button := add_button(field_container, "", open_driver.bind(str(field.id)))
			button.add_theme_font_size_override("font_size", 20)
			drive_buttons[field.id] = button

func open_driver(field: String) -> void:
	var driver: Dictionary = manager.field_driver(target_id, field)
	if not driver.is_empty(): open_script(str(driver.id))

func open_script(id: String) -> void:
	finish_all()
	requested_script = id if not id.is_empty() else "*"

func choose_target(id: String) -> void:
	finish_all()
	pending_target = manager.app.address_of(manager.app.objects[id]) if manager.app.objects.has(id) else null
	target_change_pending = true

func value_text(value: Variant, kind: String) -> String:
	if kind == "quat" and value is Array:
		var angles := Quaternion(value[0],value[1],value[2],value[3]).get_euler()
		return "%.1f°, %.1f°, %.1f°" % [rad_to_deg(angles.x),rad_to_deg(angles.y),rad_to_deg(angles.z)]
	if value is Array:
		return ", ".join(value.map(func(v): return String.num(float(v), 3)))
	return str(value)

func apply_dropped_value(field: String, value: Variant) -> void:
	if not controls.has(field) or not manager.field_driver(target_id,field).is_empty(): return
	var data: Dictionary = manager.field_data(target_id,field)
	filling = true
	var values: Variant = value
	if data.value_type == "quat":
		var angles := Quaternion(value[0],value[1],value[2],value[3]).get_euler()
		values = [rad_to_deg(angles.x),rad_to_deg(angles.y),rad_to_deg(angles.z)]
	for i in controls[field].size():
		if controls[field][i] is LineEdit:
			controls[field][i].text = str(values[i]) if values is Array else str(values)
	filling = false
	changed(field,controls[field][0],true)

func refresh_drives() -> void:
	for field in drive_labels:
		var driver: Dictionary = manager.field_driver(target_id, field)
		drive_labels[field].visible = not driver.is_empty()
		drive_buttons[field].visible = not driver.is_empty()
		if driver.is_empty(): continue
		var data: Dictionary = manager.field_data(target_id, field)
		drive_labels[field].text = "Local Drive · " + ("running" if driver.enabled else "paused") + "\nShared base: " + value_text(data.value, data.value_type)
		drive_buttons[field].text = "Open " + driver.schema.name + " →"
		if edits.has(field):
			edits[field].dirty = false
			edits[field].release = true

func playback(action: String) -> void:
	var field: Dictionary = manager.field_data(target_id, "playback")
	if field.is_empty(): return
	manager.command({"kind":"playback","command":{"target":manager.app.address_of(manager.app.objects[target_id]),"expected_revision":field.revision,"action":action}})

func refresh(object: Dictionary) -> void:
	panel_state = object
	filling = true
	pin_button.text = "Unpin" if object.panel.get("attachment") != null else "Pin"
	var entries: Array = []
	for id in manager.app.objects:
		entries.append([id, manager.field_data(id, "name").get("value", id)])
	var signature := JSON.stringify(entries)
	if signature != picker_signature:
		picker_signature = signature
		target_picker.clear()
		picker_ids.clear()
		target_picker.add_item("Choose object…")
		picker_ids.append("")
		for entry in entries:
			target_picker.add_item(entry[1])
			picker_ids.append(entry[0])
	var new_target: String = object.inspector.target.instance_id if object.inspector.get("target") != null else ""
	var inspected: Dictionary = manager.app.objects.get(new_target, {})
	var next_signature := JSON.stringify([inspected.get("fields", []).map(func(f): return [f.component_id,f.id,f.value_type]),inspected.get("scripts", []).map(func(s): return [s.id,s.schema.name,s.enabled])])
	if new_target != target_id or not built or (next_signature != field_signature and edits.is_empty()):
		for field in edits:
			# A target switch ends the old lease; never redirect a draft to the new target.
			manager.session_input(panel_id, target_id, field, "end", {}, Callable(), input_client_id)
		edits.clear()
		scroll_pending = -1.0
		target_id = new_target
		field_signature = next_signature
		build_fields()
		built = true
	target_picker.select(maxi(0, picker_ids.find(target_id)))
	if hierarchy != null: hierarchy.refresh()
	fill_values()
	refresh_drives()
	restore_scroll(object)
	filling = false
	refresh_sessions()
	if not pending_field.is_empty() and target_id == pending_reference_target and not target_change_pending and reference_chips.has(pending_field):
		scroll.ensure_control_visible(reference_chips[pending_field])
		pending_field = ""
	var show_scripts: bool = object.inspector.get("scripts_open",false)
	script_mount.visible = show_scripts
	scroll.visible = not show_scripts
	scripts_button.text = "Object fields" if show_scripts else "Scripts"
	script_view.active = show_scripts
	if show_scripts: script_view.refresh(target_id, object.inspector.get("script_id"))

func restore_scroll(object: Dictionary) -> void:
	var bar := scroll.get_v_scroll_bar()
	if scroll_pending < 0 and not scroll_inflight:
		scroll.scroll_vertical = int(float(object.inspector.scroll[1]) * maxf(0.0, bar.max_value - bar.page))

func fill_values() -> void:
	for field in reference_values:
		var value: Variant = manager.field_data(target_id,field).get("value")
		reference_values[field].text = "World" if value == null else str(manager.field_data(value.instance_id,"name").get("value","Deleted Slot"))
	for field in controls:
		if edits.has(field): continue
		var data: Dictionary = manager.field_data(target_id, field)
		if data.is_empty(): continue
		var value: Variant = manager.app.driven_objects.get(target_id, {}).get(field, data.value)
		var parts: Array = controls[field]
		if field == "collider":
			parts[0].button_pressed = value != null
			parts[1].button_pressed = value != null and value.solid
			for i in range(3): parts[i+2].text = String.num(float(value.half_extents[i]) if value != null else 0.5, 4)
		elif data.value_type == "string": parts[0].text = str(value)
		elif data.value_type == "quat":
			var angles := Quaternion(value[0],value[1],value[2],value[3]).get_euler()
			for i in range(3): parts[i].text = String.num(rad_to_deg(angles[i]), 3)
		elif value is Array:
			for i in range(parts.size()): parts[i].text = String.num(float(value[i]), 4)

func start_edit(field: String, control: Control, one_shot: bool = false) -> void:
	if filling or not manager.app.editable or target_id.is_empty(): return
	if not manager.field_driver(target_id, field).is_empty(): return
	if edits.has(field):
		edits[field].release = one_shot
		return
	var data: Dictionary = manager.field_data(target_id, field)
	if data.is_empty(): return
	edits[field] = {"granted":false,"dirty":false,"inflight":false,"release":one_shot,"revision":str(data.revision),"due":0,"heartbeat":0,"control":control,"value":null}
	manager.session_input(panel_id, target_id, field, "begin", draft_for(control), on_lease.bind(field), input_client_id)

func draft_for(control: Control) -> Dictionary:
	if control is LineEdit:
		var selection: Array = [control.get_selection_from_column(),control.get_selection_to_column()] if control.has_selection() else [control.caret_column,control.caret_column]
		return {"part":str(control.get_meta("part")),"text":control.text,"caret":control.caret_column,"selection":selection}
	return {"part":"check","text":"","caret":0,"selection":[0,0]}

func on_lease(result: Dictionary, field: String) -> void:
	if not edits.has(field): return
	if result.ok:
		edits[field].granted = true
		edits[field].heartbeat = Time.get_ticks_msec()
	else:
		edits.erase(field)
		message.text = str(result.get("message", "Another editor is using this field"))
	refresh_sessions()

func changed(field: String, control: Control, one_shot: bool = false) -> void:
	if filling: return
	start_edit(field, control, one_shot)
	if not edits.has(field): return
	edits[field].control = control
	edits[field].due = Time.get_ticks_msec() + 180
	var parsed := read_value(field)
	edits[field].dirty = parsed.ok
	edits[field].value = parsed.value

func read_value(field: String) -> Dictionary:
	var parts: Array = controls[field]
	var type: String = manager.field_data(target_id, field).get("value_type", "")
	if type == "string": return {"ok":not parts[0].text.strip_edges().is_empty(),"value":parts[0].text}
	if type == "collider" and not parts[0].button_pressed: return {"ok":true,"value":null}
	var values := []
	for control in parts:
		if not control is LineEdit: continue
		if not control.text.is_valid_float(): return {"ok":false,"value":null}
		values.append(control.text.to_float())
	if type == "quat":
		var q := Quaternion.from_euler(Vector3(deg_to_rad(values[0]),deg_to_rad(values[1]),deg_to_rad(values[2])))
		return {"ok":true,"value":[q.x,q.y,q.z,q.w]}
	if type == "collider": return {"ok":true,"value":{"solid":parts[1].button_pressed,"half_extents":values}}
	return {"ok":true,"value":values}

func edit_result(result: Dictionary, field: String) -> void:
	if not edits.has(field): return
	edits[field].inflight = false
	if result.ok:
		edits[field].revision = str(manager.field_data(target_id, field).get("revision", edits[field].revision))
		message.text = "Shared change applied"
	else:
		edits[field].dirty = false
		edits[field].release = true
		message.text = str(result.get("message", "Edit rejected"))

func tick() -> void:
	script_view.tick()
	refresh_drives()
	if not drive_labels.is_empty():
		filling = true
		fill_values()
		filling = false
	if not manager.app.editable: return
	var now := Time.get_ticks_msec()
	if scroll_pending >= 0 and not scroll_inflight and now >= scroll_due:
		var value := scroll_pending
		scroll_pending = -1.0
		scroll_inflight = true
		manager.edit(panel_id, "panel_scroll", [0.0,value], func(result):
			scroll_inflight = false
			if not result.ok: message.text = "Scroll changed elsewhere; scroll again")
	for field in edits.keys():
		var state: Dictionary = edits[field]
		if not state.granted: continue
		if state.dirty and not state.inflight and now >= state.due:
			state.dirty = false
			state.inflight = true
			manager.edit(target_id, field, state.value, edit_result.bind(field), state.revision)
		if state.release and not state.dirty and not state.inflight:
			manager.session_input(panel_id, target_id, field, "end", {}, Callable(), input_client_id)
			edits.erase(field)
			continue
		if now - state.heartbeat >= 120:
			state.heartbeat = now
			manager.session_input(panel_id, target_id, field, "update", draft_for(state.control), func(result):
				if not result.ok and edits.has(field):
					edits.erase(field)
					message.text = str(result.get("message", "Editing expired")), input_client_id)
	if edits.is_empty() and not script_view.busy() and close_when_ready:
		close_when_ready = false
		manager.close_panel(panel_id)
		return
	if edits.is_empty() and not script_view.busy() and target_change_pending:
		target_change_pending = false
		manager.edit(panel_id, "inspector_target", pending_target)
	if edits.is_empty() and not script_view.busy() and not requested_script.is_empty():
		var id := requested_script
		requested_script = ""
		if id == "*": manager.edit(panel_id,"inspector_scripts_open",true)
		else:
			manager.edit(panel_id,"inspector_script_id",id,func(result):
				if result.ok: manager.edit(panel_id,"inspector_scripts_open",true))
	refresh_sessions()

func has_active_edits() -> bool:
	return not edits.is_empty() or script_view.busy()

func finish_all() -> void:
	if not script_view.busy(): script_view.leave()
	for field in edits: edits[field].release = true

func disconnected() -> void:
	script_view.disconnected()
	edits.clear()
	scroll_pending = -1.0
	scroll_inflight = false
	target_change_pending = false
	close_when_ready = false
	message.text = "Disconnected · shared editing paused"
	refresh_sessions()

func refresh_sessions() -> void:
	var previously_remote := not remote_fields.is_empty()
	remote_fields.clear()
	for field in controls:
		var remote: Dictionary = {}
		for session in manager.sessions:
			if session.target.instance_id == target_id and session.field_id == field and (session.client_id != input_client_id or session.get("context") == null or session.context.source.instance_id != panel_id):
				remote = session
		var readonly: bool = not manager.app.editable or not remote.is_empty() or not manager.field_driver(target_id, field).is_empty()
		for control in controls[field]:
			if control is LineEdit: control.editable = not readonly
			elif control is BaseButton: control.disabled = readonly
		if not remote.is_empty():
			remote_fields[field] = true
			var owner: String = str(remote.owner)
			var display_name: String = str(manager.app.participant_names.get("local" if owner.begins_with("local-") else owner, "Another user"))
			message.text = "Editing: " + display_name + " · " + field
			for control in controls[field]:
				if control is LineEdit and str(control.get_meta("part")) == str(remote.draft.part):
					filling = true
					control.text = remote.draft.text
					control.caret_column = int(remote.draft.caret)
					control.select(int(remote.draft.selection[0]),int(remote.draft.selection[1]))
					filling = false
	if previously_remote and remote_fields.is_empty():
		filling = true
		fill_values()
		filling = false
		if message.text.begins_with("Editing:"): message.text = "Shared panel · FPS: RMB hold · E rotate"

func on_scroll(_value: float) -> void:
	if filling or panel_state.is_empty() or not manager.app.editable: return
	var bar := scroll.get_v_scroll_bar()
	var ratio: float = float(scroll.scroll_vertical) / maxf(1.0, bar.max_value - bar.page)
	scroll_pending = clampf(ratio,0.0,1.0)
	scroll_due = Time.get_ticks_msec() + 80

func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE and script_view != null:
		script_view.dispose()
