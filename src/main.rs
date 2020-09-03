fn main() {
    zx::run(r"fun x -> if x then true else { a = true }");
}
