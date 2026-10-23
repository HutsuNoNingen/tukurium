extends SceneTree
const BROKER = preload("res://request_broker.gd")
var sent: Array = []
var completed: Array = []
var clock := 0

func _initialize() -> void:
	var broker = BROKER.new()
	broker.send = func(body): sent.append(body)
	broker.now = func(): return clock
	var a: String = broker.request({"kind":"edit","edit":{}},func(body): completed.append(["a",body]),["edit","command_id"])
	var b: String = broker.request({"kind":"script","command":{}},func(body): completed.append(["b",body]))
	assert(not broker.receive({"kind":"pose","command_id":a}))
	assert(broker.receive({"kind":"result","command_id":b,"ok":true}))
	assert(broker.receive({"kind":"result","command_id":a,"ok":true}))
	assert(not broker.receive({"kind":"result","command_id":a,"ok":true}))
	assert(completed.size()==2 and completed[0][0]=="b" and completed[1][0]=="a")
	broker.request({"kind":"test","command":{}},func(body): completed.append(["timeout",body]))
	clock = 11000
	broker.tick()
	assert(completed[-1][0]=="timeout" and completed[-1][1].ok==false)
	var old: String = broker.request({"kind":"test","command":{}},func(body): completed.append(["disconnect",body]))
	broker.disconnected()
	assert(completed[-1][0]=="disconnect" and not broker.receive({"kind":"result","command_id":old}))
	assert(sent.size()==4)
	print("REQUEST BROKER PASS: correlation, duplicate replies, timeout and disconnect without replay")
	quit()
