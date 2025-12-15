fn main() {
    let mut y: i32 = 4;
    let z = &mut y;
    *z += 10;

    let p : *mut i32 = z;
    let p2 : *mut i32 = z;

    *z += 10;

    unsafe {
        *p = 10; // Write to z invalidates both its children
        *p2 = 20; 
    }
   
}
