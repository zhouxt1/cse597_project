fn main() {
    let mut y: i32 = 4;
    let z = &mut y;
    *z += 10;

    let p : *mut i32 = z;
    let p2 : *mut i32 = z;

    unsafe {
        *p = 10; // there is NO UB in this case. Since p and p2 are both raw pointer, they are in the same class
        *p2 = 20; 
    }
   
}
