extends Control
var view

func _process(_delta: float) -> void:
	queue_redraw()

func _draw() -> void:
	if view == null or view.document.is_empty(): return
	var font := ThemeDB.fallback_font
	for cursor in view.document.get("cursors", []):
		if str(cursor.client_id).ends_with("/" + view.client_id): continue
		var point: Vector2i = view.line_column(view.editor.code.text, int(cursor.head))
		var position: Vector2 = view.editor.code.get_pos_at_line_column(point.x,point.y)
		if position.x < 0 or position.y < 0: continue
		var color := Color.from_hsv(float(absi(str(cursor.client_id).hash()) % 360) / 360.0, 0.55, 1.0)
		draw_line(position-Vector2(0,18),position,color,2.0)
		draw_string(font,position+Vector2(3,-18),str(cursor.name),HORIZONTAL_ALIGNMENT_LEFT,-1,13,color)
