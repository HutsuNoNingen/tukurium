extends Control
var view
var drawn_signature := ""
var signature_known := false

func _process(_delta: float) -> void:
	if not is_visible_in_tree(): return
	var signature := _draw_signature()
	if signature_known and signature == drawn_signature: return
	signature_known = true
	drawn_signature = signature
	queue_redraw()

# Everything _draw() reads: remote carets, the text they index into, and the
# editor geometry that maps offsets to pixels. Redraw only when one changes.
func _draw_signature() -> String:
	if view == null or view.document.is_empty(): return ""
	var code = view.editor.code
	var parts: Array = [str(view.client_id), str(code.text.hash()), str(code.scroll_vertical), str(code.scroll_horizontal), str(code.size)]
	for cursor in view.document.get("cursors", []):
		parts.append("%s|%d|%s" % [str(cursor.client_id), int(cursor.head), str(cursor.name)])
	return "\n".join(parts)

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
