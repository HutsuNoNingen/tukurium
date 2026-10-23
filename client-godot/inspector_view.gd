extends PanelContainer
## One inspector instance. Selection, drafts and requests never use the HUD's singleton state.
var manager
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

func setup(controller, id: String) -> void:
	manager = controller
	panel_id = id
	theme = Theme.new()
	theme.default_font_size = 26
	var style := StyleBoxFlat.new()
	style.bg_color = Color("15212e")
	style.border_color = Color("5ac8c5")
	style.set_border_width_all(3)
	style.set_corner_radius_all(10)
	add_theme_stylebox_override("panel", style)
	var margin := MarginContainer.new()
	for side in ["left", "top", "right", "bottom"]: margin.add_theme_constant_override("margin_"+side, 16)
	add_child(margin)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 12)
	margin.add_child(column)
	var header := HBoxContainer.new()
	column.add_child(header)
	title.text = "SHARED INSPECTOR"
	title.add_theme_font_size_override("font_size", 27)
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(title)
	add_button(header, "+", func(): manager.duplicate_panel(panel_id))
	pin_button = add_button(header, "Pin", func(): manager.pin_panel(panel_id))
	add_button(header, "×", func(): manager.close_panel(panel_id))
	column.add_child(target_picker)
	target_picker.item_selected.connect(func(index: int):
		if filling or index >= picker_ids.size(): return
		finish_all()
		var id_value: String = picker_ids[index]
		var target: Variant = manager.app.address_of(manager.app.objects[id_value]) if manager.app.objects.has(id_value) else null
		pending_target = target
		target_change_pending = true)
	var actions := HBoxContainer.new()
	column.add_child(actions)
	add_button(actions, "Select / Gizmo", func():
		if not target_id.is_empty(): manager.app.select_object(target_id))
	add_button(actions, "Scripts…", func():
		if not target_id.is_empty():
			manager.app.release_cursor()
			manager.app.script_editor.open(target_id))
	add_button(actions, "New inspector", func(): manager.open_inspector(target_id))
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	column.add_child(scroll)
	fields_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	fields_box.add_theme_constant_override("separation", 10)
	scroll.add_child(fields_box)
	scroll.get_v_scroll_bar().value_changed.connect(on_scroll)
	message.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	message.add_theme_font_size_override("font_size", 18)
	message.text = "Shared panel · FPS: RMB hold · Wheel distance · E rotate"
	column.add_child(message)

func add_button(parent: Node, text: String, action: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.focus_mode = Control.FOCUS_NONE
	button.pressed.connect(action)
	parent.add_child(button)
	return button

func heading(text: String) -> void:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", Color("9cc6d4"))
	fields_box.add_child(label)

func line_field(parent: Node, field: String, part: String) -> LineEdit:
	var line := LineEdit.new()
	line.custom_minimum_size = Vector2(0, 44)
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
	fields_box.add_child(row)
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
	if target_id.is_empty() or not manager.app.objects.has(target_id):
		heading("Choose an object above. Deleted targets are cleared.")
		return
	for field in manager.app.objects[target_id].fields:
		match field.value_type:
			"string":
				heading(field.label)
				line_field(fields_box, field.id, "0")
			"vec3": vector_field(field.id, field.label, ["X", "Y", "Z"])
			"quat": vector_field(field.id, field.label + " · degrees", ["X", "Y", "Z"])
			"color": vector_field(field.id, field.label, ["R", "G", "B", "A"])
			"collider":
				heading("Collider")
				var row := HBoxContainer.new()
				fields_box.add_child(row)
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
					fields_box.add_child(row)
					add_button(row, "Play", func(): playback("play"))
					add_button(row, "Pause", func(): playback("pause"))
					add_button(row, "Restart", func(): playback("restart"))
			_:
				heading(field.label + " · " + JSON.stringify(field.value))

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
	if new_target != target_id or not built:
		for field in edits:
			# A target switch ends the old lease; never redirect a draft to the new target.
			manager.session_input(panel_id, target_id, field, "end", {})
		edits.clear()
		scroll_pending = -1.0
		target_id = new_target
		build_fields()
		built = true
	target_picker.select(maxi(0, picker_ids.find(target_id)))
	fill_values()
	var bar := scroll.get_v_scroll_bar()
	if scroll_pending < 0 and not scroll_inflight:
		scroll.scroll_vertical = int(float(object.inspector.scroll[1]) * maxf(0.0, bar.max_value - bar.page))
	filling = false
	refresh_sessions()

func fill_values() -> void:
	for field in controls:
		if edits.has(field): continue
		var data: Dictionary = manager.field_data(target_id, field)
		if data.is_empty(): continue
		var value: Variant = data.value
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
	if edits.has(field):
		edits[field].release = one_shot
		return
	var data: Dictionary = manager.field_data(target_id, field)
	if data.is_empty(): return
	edits[field] = {"granted":false,"dirty":false,"inflight":false,"release":one_shot,"revision":str(data.revision),"due":0,"heartbeat":0,"control":control,"value":null}
	manager.session_input(panel_id, target_id, field, "begin", draft_for(control), on_lease.bind(field))

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
			manager.session_input(panel_id, target_id, field, "end", {})
			edits.erase(field)
			continue
		if now - state.heartbeat >= 120:
			state.heartbeat = now
			manager.session_input(panel_id, target_id, field, "update", draft_for(state.control), func(result):
				if not result.ok and edits.has(field):
					edits.erase(field)
					message.text = str(result.get("message", "Editing expired")))
	if edits.is_empty() and close_when_ready:
		close_when_ready = false
		manager.close_panel(panel_id)
		return
	if edits.is_empty() and target_change_pending:
		target_change_pending = false
		manager.edit(panel_id, "inspector_target", pending_target)
	refresh_sessions()

func finish_all() -> void:
	for field in edits: edits[field].release = true

func disconnected() -> void:
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
			if session.target.instance_id == target_id and session.field_id == field and (session.client_id != manager.client_id or session.get("context") == null or session.context.source.instance_id != panel_id):
				remote = session
		var readonly: bool = not manager.app.editable or not remote.is_empty()
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
