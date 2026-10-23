extends Control
var kind := "cube"

func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO,size),Color("eaeef5"))
	var c := size*0.5
	var u := minf(size.x,size.y)*0.22
	var ink := Color("7890b3")
	if kind=="image" or kind=="video":
		draw_rect(Rect2(c-Vector2(u*1.4,u),Vector2(u*2.8,u*2)),ink,false,2)
		if kind=="video": draw_colored_polygon(PackedVector2Array([c+Vector2(-u*.3,-u*.5),c+Vector2(u*.55,0),c+Vector2(-u*.3,u*.5)]),ink)
		else:
			draw_polyline(PackedVector2Array([c+Vector2(-u,u*.55),c+Vector2(-u*.25,-u*.3),c+Vector2(u*.25,u*.2),c+Vector2(u*.7,-u*.15),c+Vector2(u,u*.55)]),ink,2,true)
			draw_circle(c+Vector2(u*.6,-u*.55),u*.14,ink)
	else:
		var top := c-Vector2(0,u)
		var left := c+Vector2(-u,-u*.4)
		var right := c+Vector2(u,-u*.4)
		var middle := c+Vector2(0,u*.15)
		var bottom := c+Vector2(0,u*1.25)
		draw_colored_polygon(PackedVector2Array([top,right,middle,left]),Color("f8faff"))
		draw_colored_polygon(PackedVector2Array([left,middle,bottom,left+Vector2(0,u*1.1)]),Color("cad7e9"))
		draw_colored_polygon(PackedVector2Array([middle,right,right+Vector2(0,u*1.1),bottom]),Color("b6c8e1"))
		draw_polyline(PackedVector2Array([top,right,right+Vector2(0,u*1.1),bottom,left+Vector2(0,u*1.1),left,top,middle,bottom]),ink,2,true)
		draw_line(left,middle,ink,2,true)
		draw_line(middle,right,ink,2,true)

func _notification(what: int) -> void:
	if what==NOTIFICATION_RESIZED: queue_redraw()
