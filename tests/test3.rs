fn main() {
    let mut y: i32 = 4;
    let z = &mut y;
    *z += 10;

    let p : *mut i32 = z;
    unsafe {
        let x: &mut i32 = &mut *p;
        let y: &mut i32 = &mut *p;

        *x = 22;  // UB since x's parent *p is used when creating y
        *y = 25; // also UB
    }
}
