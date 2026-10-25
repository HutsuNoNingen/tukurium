extends RefCounted
## Render the shared primitive descriptor without downloading mesh assets.
const SHAPES := ["box","sphere","capsule","cylinder","cone","torus","grid","quad"]
const NAMES := ["Box","Sphere","Capsule","Cylinder","Cone","Torus","Grid","Quad"]

static func create(shape: String) -> PrimitiveMesh:
	match shape:
		"sphere":
			var mesh := SphereMesh.new()
			mesh.radius=0.75; mesh.height=1.5
			return mesh
		"capsule":
			var mesh := CapsuleMesh.new()
			mesh.radius=0.375; mesh.height=1.5
			return mesh
		"cylinder", "cone":
			var mesh := CylinderMesh.new()
			mesh.bottom_radius=0.75; mesh.top_radius=0.0 if shape=="cone" else 0.75; mesh.height=1.5
			return mesh
		"torus":
			var mesh := TorusMesh.new()
			mesh.inner_radius=0.35; mesh.outer_radius=0.75
			return mesh
		"grid":
			var mesh := PlaneMesh.new()
			mesh.size=Vector2(1.5,1.5); mesh.subdivide_width=8; mesh.subdivide_depth=8
			return mesh
		"quad", "mirror":
			var mesh := QuadMesh.new()
			mesh.size=Vector2(2.0,2.0) if shape=="mirror" else Vector2(1.5,1.5)
			return mesh
		"camera":
			var mesh := BoxMesh.new()
			mesh.size=Vector3(0.8,0.5,0.9)
			return mesh
		_:
			var mesh := BoxMesh.new()
			mesh.size=Vector3.ONE*1.5
			return mesh
