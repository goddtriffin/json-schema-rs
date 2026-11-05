//! Build script of every reverse-codegen model shard: writes the shard's models (see
//! `reverse_model_gen`).

fn main() {
    reverse_model_gen::build_shard();
}
