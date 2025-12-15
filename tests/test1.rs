fn main() {
    let mut y: i32 = 4;
    let z = &mut y;
    *z += 10;

    let x: &mut i32 = &mut *z;

    let p : *mut i32 = x;

    *z += 22; // z is used, which is a grandparent of x, invalidates the pointer p

    unsafe { 
        *p = 20;
    }

}