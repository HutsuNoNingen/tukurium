extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	var saw_draft := false
	var saw_commit := false
	var selected := ""
	for _i in range(1000):
		await create_timer(0.01).timeout
		if scene.world_panels.panels.size() != 2: continue
		if selected.is_empty():
			selected = scene.selected_id
			print("PANEL OBSERVER READY")
		if selected != scene.selected_id:
			push_error("Remote panel changed the observer's selected object"); quit(1); return
		for session in scene.world_panels.sessions:
			if session.draft.text != "World panel edit ": continue
			var view = scene.world_panels.panels[session.context.source.instance_id].content
			var line: LineEdit = view.controls.name[0]
			if line.editable or line.text != session.draft.text or line.caret_column != session.draft.caret:
				push_error("Remote draft/caret/read-only state differ"); quit(1); return
			saw_draft = true
		for object in scene.objects.values():
			for field in object.fields:
				if field.id == "name" and field.value == "World panel edit ": saw_commit = true
		if saw_draft and saw_commit:
			print("PANEL OBSERVER PASS: remote draft, caret, field ownership and independent selection")
			scene.queue_free()
			await process_frame
			await process_frame
			quit(0)
			return
	push_error("Observer did not receive the shared edit")
	quit(1)
