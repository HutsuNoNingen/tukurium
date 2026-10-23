extends RefCounted
## Correlates bounded one-shot requests. State changes are never replayed after reconnect.
var send: Callable
var now: Callable = func(): return Time.get_ticks_msec()
var requests: Dictionary = {}
var generation := 0
var sequence := 0
var prefix := "%s-%s" % [Time.get_ticks_usec(), randi()]
const MAX_PENDING := 128

func request(body: Dictionary, completed: Callable, id_path: Array = ["command", "command_id"], response_id: String = "command_id", timeout_ms: int = 10000, response_kind: String = "result") -> String:
	sequence += 1
	var id := "rpc-%s-%s-%s" % [prefix,generation,sequence]
	if requests.size() >= MAX_PENDING:
		_reject.call_deferred(completed, id, "Too many pending requests")
		return id
	var payload := body.duplicate(true)
	var container: Dictionary = payload
	for key in id_path.slice(0, id_path.size()-1):
		if not container.has(key): container[key] = {}
		container = container[key]
	container[id_path[-1]] = id
	requests[id] = {"completed":completed,"response_id":response_id,"response_kind":response_kind,"deadline":int(now.call())+timeout_ms}
	send.call(payload)
	return id

func receive(body: Dictionary) -> bool:
	for key in ["command_id", "request_id"]:
		var id := str(body.get(key, ""))
		if not requests.has(id) or requests[id].response_id != key or requests[id].response_kind != body.get("kind"): continue
		var callback: Callable = requests[id].completed
		requests.erase(id)
		if callback.is_valid(): callback.call(body)
		return true
	return false

func tick() -> void:
	for id in requests.keys():
		if int(now.call()) < requests[id].deadline: continue
		var callback: Callable = requests[id].completed
		requests.erase(id)
		_reject(callback,id,"Request timed out. Review the shared state before retrying.")

func disconnected() -> void:
	generation += 1
	var old := requests
	requests = {}
	for id in old:
		_reject(old[id].completed,id,"Connection interrupted. Review the shared state before retrying.")

func _reject(callback: Callable, id: String, message: String) -> void:
	if callback.is_valid(): callback.call({"kind":"result","command_id":id,"request_id":id,"ok":false,"message":message})
