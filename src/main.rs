use handshake::Handshake;

fn main() {
    let (u, v) = Handshake::<Box<str>>::new();
    let combine = |x, y| format!("{} {}!", x, y);

    '_task_a: {
        u.join("Handle Communication".into(), combine)
            .unwrap()
            .map(|s| println!("{}", s));
    }

    '_task_b: {
        v.join("Symmetrically".into(), combine)
            .unwrap()
            .map(|s| println!("{}", s));
    }
}