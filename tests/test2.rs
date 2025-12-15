fn main() {
    let mut y: i32 = 4;
    let z = &mut y;
    *z += 10;

    let x: &mut i32 = &mut *z;

    let p : *mut i32 = x;

    *x = 22; // x is used, which invalidates the pointer p
            // Note that this is not a UB in Tree Borrow

    unsafe { 
        *p = 20;
    }

}