// Scatter upload (crates/rook-render/src/batch/scatter.rs): copies the
// instances that changed this frame into their slots in the instance
// buffer, so a frame uploads only what changed rather than every chunk
// that holds a change.

// Words in one instance.
override WORDS: u32;

// The count, then per instance its slot and its words.
@group(0) @binding(0) var<storage, read> changed: array<u32>;
@group(0) @binding(1) var<storage, read_write> instances: array<u32>;

@compute @workgroup_size(64)
fn scatter(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= changed[0] {
        return;
    }
    let at = 1u + i * (WORDS + 1u);
    let slot = changed[at] * WORDS;
    for (var w = 0u; w < WORDS; w++) {
        instances[slot + w] = changed[at + 1u + w];
    }
}
