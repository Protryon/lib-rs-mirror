use ructe::Ructe;

fn main() {
    Ructe::from_env().unwrap().compile_templates("templates").unwrap();
}
