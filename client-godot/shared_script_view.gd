extends RefCounted
## Collaborative draft adapter for the same editor used by the desktop window.
const EDITOR = preload("res://script_editor.gd")
const CARETS = preload("res://script_carets.gd")
var manager
var recovery_text := ""
var active := false
var deferred_target: Dictionary = {}
var panel_id := ""
var client_id := ""
var editor
var target_id := ""
var document: Dictionary = {}
var latest: Dictionary = {}
var flight: Dictionary = {}
var pending_action: Dictionary = {}
var opening := false
var due := 0
var heartbeat := 0
var generation := 0
var selecting := false
var local_selection := false
var overlay

func setup(controller, id: String, mount: Control) -> void:
	manager = controller
	panel_id = id
	client_id = manager.client_id + "/" + id + "/" + str(get_instance_id())
	editor = EDITOR.new()
	editor.setup(manager.app, mount)
	editor.shared_document_mode = true
	editor.action_guard = authorize
	editor.selection_guard = can_select
	editor.selection_changed = select_shared
	editor.draft_changed = func(): due = Time.get_ticks_msec() + 80
	editor.reload_button.text = "Reload draft"
	editor.reload_guard = func():
		if not flight.is_empty():
			editor.feedback.text = "Wait for the draft to finish synchronizing."
		else:
			recovery_text = ""
			pending_action = {}
			open_document()
	editor.code.caret_changed.connect(func(): heartbeat = 0)
	overlay = CARETS.new()
	overlay.view = self
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.clip_contents = true
	editor.code.add_child(overlay)

func request(action: String, extra: Dictionary, callback: Callable) -> void:
	if not manager.app.objects.has(target_id) or editor.selected.is_empty(): return
	var input := {"target":manager.app.address_of(manager.app.objects[target_id]), "script_id":editor.selected,
		"client_id":client_id,"action":action,"generation":document.get("generation", ""),"name":manager.app.nickname.text}
	input.merge(extra)
	manager.app.requests.request({"kind":"script_document","request":input},callback,["request","request_id"],"request_id",10000,"script_document_result")

func refresh(target: String, selected: Variant) -> void:
	if not recovery_text.is_empty():
		editor.external_readonly = true
		editor.refresh_buttons()
		editor.feedback.text = "Unsynced text preserved. Copy it before Reload draft."
		return
	if (target != target_id or (selected is String and selected != editor.selected)) and busy():
		deferred_target = {"target":target,"selected":selected}
		editor.external_readonly = true
		editor.refresh_buttons()
		return
	if target != target_id:
		leave()
		target_id = target
		editor.open(target)
	if selected is String and not selecting and selected != editor.selected and can_select() and manager.app.objects.get(target_id, {}).get("scripts", []).any(func(item): return item.id == selected):
		leave()
		editor.selected = selected
		editor.source_dirty = false
		editor.config_signature = ""
	editor.refresh()
	editor.refresh()
	var current: Dictionary = editor.current_script()
	if current.is_empty():
		leave()
		return
	if document.get("script_id", "") != editor.selected and not opening:
		leave()
		open_document()
	elif not document.is_empty():
		editor.base_revision = current.revision
		if flight.is_empty() and editor.code.text == document.text: display(document.text, caret_offset())

func can_select() -> bool:
	if not flight.is_empty() or (not document.is_empty() and editor.code.text != document.text):
		editor.feedback.text = "Synchronizing the draft before switching…"
		return false
	return true

func select_shared(id: String) -> void:
	leave()
	if not local_selection and not selecting:
		selecting = true
		manager.edit(panel_id,"inspector_script_id",null if id.is_empty() else id,func(_r): selecting = false)
	open_document()

func open_document() -> void:
	if editor.selected.is_empty() or opening or not manager.app.objects.has(target_id): return
	generation += 1
	var token := generation
	opening = true
	editor.external_readonly = true
	editor.refresh_buttons()
	request("open",{},func(result):
		if token != generation: return
		opening = false
		if not result.ok:
			editor.feedback.text = result.get("message","Cannot open shared draft")
			return
		document = result.document
		latest = document
		editor.external_readonly = false
		display(document.text, 0)
		editor.feedback.text = "Shared draft · everyone can type · Apply code to run"
		heartbeat = 0)

func receive(value: Dictionary) -> void:
	if value.target.instance_id != target_id or value.script_id != editor.selected: return
	if document.is_empty(): return
	if value.generation != document.generation:
		editor.external_readonly = true
		editor.refresh_buttons()
		editor.feedback.text = "The document was reset. Reload draft to continue."
		return
	if int(value.revision) < int(latest.get("revision",0)): return
	latest = value
	if not flight.is_empty(): return
	if editor.code.text != document.text:
		flush()
		return
	var offset := remap_caret(document.ids, value.ids, caret_offset())
	document = value
	display(document.text, offset)

func delta(before: String, after: String, ids: Array) -> Dictionary:
	var prefix := 0
	while prefix < mini(before.length(),after.length()) and before[prefix] == after[prefix]: prefix += 1
	var suffix := 0
	while suffix < mini(before.length()-prefix,after.length()-prefix) and before[before.length()-suffix-1] == after[after.length()-suffix-1]: suffix += 1
	return {"after":"0" if prefix == 0 else ids[prefix-1],"delete":ids.slice(prefix,before.length()-suffix),"insert":after.substr(prefix,after.length()-prefix-suffix)}

func project(base: Dictionary, change: Dictionary, previous_ids: Array) -> Dictionary:
	var ids: Array = []
	var chars: Array = []
	for i in base.ids.size():
		if not change.delete.has(base.ids[i]):
			ids.append(base.ids[i])
			chars.append(base.text[i])
	var at := ids.find(change.after) + 1
	if change.after != "0" and at == 0:
		var old := previous_ids.find(change.after)
		while old >= 0:
			at = ids.find(previous_ids[old]) + 1
			if at > 0: break
			old -= 1
	for i in change.insert.length():
		ids.insert(at+i, "")
		chars.insert(at+i, change.insert[i])
	return {"text":"".join(chars),"ids":ids,"caret":at+change.insert.length()}

func flush() -> void:
	if opening or document.is_empty() or not flight.is_empty() or editor.code.text == document.text: return
	var change := delta(document.text, editor.code.text, document.ids)
	var proposed := project(document, change, document.ids)
	send_edit(change, proposed)

func send_edit(change: Dictionary, proposed: Dictionary) -> void:
	var token := generation
	flight = proposed
	request("edit",change,func(result):
		if token != generation: return
		if not result.ok:
			flight.clear()
			editor.external_readonly = true
			editor.refresh_buttons()
			editor.feedback.text = result.get("message","Draft synchronization failed; copy your text before reloading")
			return
		var sent := flight.duplicate(true)
		var inserted: Array = result.get("inserted_ids",[])
		var next := 0
		for i in sent.ids.size():
			if sent.ids[i] == "":
				sent.ids[i] = inserted[next]
				next += 1
		var local_text: String = editor.code.text
		var offset := caret_offset()
		flight.clear()
		if latest.is_empty() or int(result.document.revision) >= int(latest.revision): latest = result.document
		document = latest
		if sent.text == local_text:
			display(document.text, remap_caret(sent.ids, document.ids, offset))
		else:
			var queued := delta(sent.text, local_text, sent.ids)
			var rebased := project(document, queued, sent.ids)
			display(rebased.text, rebased.caret)
			send_edit(queued, rebased))

func display(text: String, offset: int) -> void:
	editor.filling = true
	if editor.code.text != text:
		editor.code.text = text
		var point := line_column(text, offset)
		editor.code.set_caret_line(point.x)
		editor.code.set_caret_column(point.y)
	editor.filling = false
	var current: Dictionary = editor.current_script()
	editor.source_dirty = not current.is_empty() and text != current.source
	if editor.feedback.text.begins_with("Draft") or editor.feedback.text.begins_with("Shared draft"):
		editor.feedback.text = "Shared draft · session only · Apply code to run/save" if editor.source_dirty else "Shared draft · synchronized with applied code"
	if not current.is_empty(): editor.base_revision = current.revision
	editor.refresh_buttons()

func offset_at(line: int, column: int) -> int:
	var offset := column
	for i in line: offset += editor.code.get_line(i).length()+1
	return offset

func caret_offset() -> int:
	return offset_at(editor.code.get_caret_line(), editor.code.get_caret_column())

func line_column(text: String, offset: int) -> Vector2i:
	var line := 0
	var column := 0
	for i in mini(offset,text.length()):
		if text[i] == "\n": line += 1; column = 0
		else: column += 1
	return Vector2i(line,column)

func remap_caret(old_ids: Array, new_ids: Array, offset: int) -> int:
	for i in range(mini(offset,old_ids.size())-1,-1,-1):
		var found := new_ids.find(old_ids[i])
		if found >= 0: return found+1
	return 0

func authorize(action: Dictionary) -> void:
	if opening or (not flight.is_empty()) or (not document.is_empty() and editor.code.text != document.text):
		pending_action = action
		flush()
		editor.feedback.text = "Synchronizing the shared draft before applying…"
		return
	if action.action == "update" and not document.is_empty() and editor.source_dirty:
		action.source = document.text
		action.expected_revision = editor.current_script().revision
	editor.dispatch_action(action)

func tick() -> void:
	editor.tick()
	if not busy() and not deferred_target.is_empty():
		var target := deferred_target
		deferred_target = {}
		editor.external_readonly = false
		refresh(target.target,target.selected)
	if not active and not busy():
		if not document.is_empty(): leave()
		return
	if document.is_empty() or opening or not manager.app.editable: return
	var now := Time.get_ticks_msec()
	if now >= due: flush()
	if flight.is_empty() and not pending_action.is_empty() and editor.code.text == document.text:
		var action := pending_action
		pending_action = {}
		authorize(action)
	if flight.is_empty() and editor.code.text == document.text and now-heartbeat > 1500:
		heartbeat = now
		var offset := caret_offset()
		var origin := offset_at(editor.code.get_selection_origin_line(),editor.code.get_selection_origin_column()) if editor.code.has_selection() else offset
		var anchor: String = document.ids[mini(origin,document.ids.size())-1] if origin > 0 and not document.ids.is_empty() else "0"
		var head: String = document.ids[mini(offset,document.ids.size())-1] if offset > 0 and not document.ids.is_empty() else "0"
		request("cursor",{"anchor":anchor,"head":head},func(result):
			if result.ok: receive(result.document))

func leave() -> void:
	if not document.is_empty() and flight.is_empty():
		manager.app.requests.request({"kind":"script_document","request":{"target":document.target,"script_id":document.script_id,"client_id":client_id,"action":"leave","generation":document.generation}},func(_r): pass,["request","request_id"],"request_id",10000,"script_document_result")
	generation += 1
	document = {}
	latest = {}
	flight = {}
	opening = false
	pending_action = {}

func busy() -> bool:
	return not flight.is_empty() or (not document.is_empty() and editor.code.text != document.text)

func disconnected() -> void:
	if busy(): recovery_text = editor.code.text
	leave()
	editor.external_readonly = true
	editor.refresh_buttons()

func dispose() -> void:
	generation += 1
	if editor != null:
		editor.action_guard = Callable()
		editor.reload_guard = Callable()
		editor.selection_guard = Callable()
		editor.selection_changed = Callable()
		editor.draft_changed = Callable()
	if is_instance_valid(overlay): overlay.view = null
