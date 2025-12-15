#![feature(rustc_private)] // Allow access to internal crates
#![feature(box_patterns)]  // Useful for matching MIR patterns

// You must explicitly declare these external crates
extern crate rustc_driver;
extern crate rustc_index;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_hir;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_mir_dataflow; // <--- The tool you need

use rustc_driver::{Callbacks, Compilation, run_compiler};
use rustc_interface::{interface};
//use rustc_middle::mir::{Body, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind};
use rustc_middle::mir::{
    //visit::{PlaceContext, Visitor},
    Local, Body, Location, Rvalue, StatementKind, Statement, BorrowKind,
    ProjectionElem
};
use rustc_middle::ty::{TyCtxt, TyKind, Mutability};
use rustc_index::{
    bit_set::MixedBitSet,
    IndexVec,
};
// Import the dataflow analysis framework
use rustc_mir_dataflow::{
    fmt::DebugWithContext, Analysis, JoinSemiLattice,
};
// For creating a compact bitset for our dataflow domain


struct MyAnalysisCallbacks;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AncestryState {
    // IndexVec; indexed by Local variables instead of usize
    // index: the child Local
    // value: the set of parent Locals
    pub ancestry: IndexVec<Local, MixedBitSet<Local>>,

    // problem: children set changes over time. 
    pub all_children: IndexVec<Local, MixedBitSet<Local>>,
    pub revoked: MixedBitSet<Local>,
}

impl JoinSemiLattice for AncestryState {
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        // Iterate over all locals in the MIR body
        for (local, other_ancestors) in other.ancestry.iter_enumerated() {
            // Union the ancestors from the other branch into this one
            changed |= self.ancestry[local].union(other_ancestors);
        }
        for (local, other_children) in other.all_children.iter_enumerated() {
            changed |= self.all_children[local].union(other_children);
        }
        changed |= self.revoked.union(&other.revoked);

        changed
    }
}

impl<C> DebugWithContext<C> for AncestryState {

}

struct AncestryAnalysis;

impl<'tcx> Analysis<'tcx> for AncestryAnalysis {
    type Domain = AncestryState;
    const NAME: &'static str = "AncestryAnalysis";
    
    fn bottom_value(&self, body: &rustc_middle::mir::Body<'tcx>) -> <Self as rustc_mir_dataflow::Analysis<'tcx>>::Domain { 
        AncestryState {
            ancestry: IndexVec::from_elem(
                MixedBitSet::new_empty(body.local_decls.len()),
                &body.local_decls,
            ),
            all_children: IndexVec::from_elem(
                MixedBitSet::new_empty(body.local_decls.len()),
                &body.local_decls,
            ),
            revoked: MixedBitSet::new_empty(body.local_decls.len()),
        }
    }

    fn initialize_start_block(
        &self,
        _body: &rustc_middle::mir::Body<'tcx>,
        state: &mut Self::Domain,
    ) {
        // At the start of the function, no locals have ancestors
        for local in state.ancestry.indices() {
            state.ancestry[local].clear();
        }
    }

    // here, the logic should be that if x = &mut y, then x's ancestors include y and y's ancestors
    // second case, if x = *mut y, then x's ancestors include y and y's ancestors
    // i need to consider a lot of other cases as well

    fn apply_primary_statement_effect(
        &self,
        state: &mut Self::Domain,
        statement: &Statement<'tcx>,
        _location: Location,
    ) {
        if let StatementKind::Assign(box (place, rvalue)) = &statement.kind {
            // Helper function to implement the recursive revocation.
            // we don't revoke the local itself, only its children!
            fn revoke_recursive(local: Local, state: &mut AncestryState) {
                // If already revoked, we don't need to do anything.
                if state.revoked.contains(local) {
                    return;
                }
                // Mark this local as revoked.
                //state.revoked.insert(local);

                // Recursively revoke all of its children.
                if let Some(children) = state.all_children.get(local).cloned() {
                    for child in children.iter() {
                        state.revoked.insert(child);
                        revoke_recursive(child, state);
                    }
                }
            }



            // We only care if we are assigning to a Local (ignoring projections for simplicity)
            if let Some(dest_local) = place.as_local() {
                let mut new_ancestors = MixedBitSet::new_empty(state.ancestry.len());
                let mut new_children = MixedBitSet::new_empty(state.all_children.len());

                // Helper to add a source local and its existing ancestors
                let mut add_source = |src_local: Local| {
                    // Add the source itself as a direct ancestor
                    new_ancestors.insert(src_local);
                    // Add the source's ancestors (transitive history)
                    if let Some(existing) = state.ancestry.get(src_local) {
                        new_ancestors.union(existing);
                    }
                };

                // if we are adding child, the 'child' is dest_local, the 'parent' is src_local
                // this child is the 'direct' children. We cannot track the all indirect deceidents. 
                // that would require updating the entire graph each time.
                let add_child = |src_local: Local| {
                    // Add dest_local as a child of src_local
                    new_children.insert(dest_local);
                    if let Some(existing) = state.all_children.get(src_local) {
                        new_children.union(existing);
                    }
                    state.all_children[src_local] = new_children;
                };

                // Revoke logic: When a local is reassigned, we must revoke all of its children
                // from the previous state, as they were derived from an now-outdated value.
                // We do this *before* calculating the new ancestry.

                match rvalue {
                    // we only care about mutable borrow now
                    // Case: `dest = &mut source;`
                    Rvalue::Ref(_, BorrowKind::Mut { .. }, borrowed_place) => {
                        // If the borrowed place is a simple local variable (e.g., `x` in `&mut x`)
                        if let Some(source_local) = borrowed_place.as_local() {
                            // Use the helper to add the source and its ancestors
                            // to the new ancestor set for `dest`.
                            add_source(source_local);
                            add_child(source_local);
                        }

                        // but another case is when borrowed_place is a deref 
                        // dest = &mut *source;
                        // note that for this case, it counts as a 'use' of the 
                        else if let [ProjectionElem::Deref] = borrowed_place.projection.as_slice() {
                            let target_local = borrowed_place.local;
                            add_source(target_local);
                            add_child(target_local);

                            // we revoke the target_local
                            revoke_recursive(target_local, state);
                        }
                    },
                    // Case: `dest = *mut source;`
                    Rvalue::RawPtr(_, source_place) => {
                        // If the source place is a simple local variable (e.g., `x` in `*mut x`)
                        // The source_place can have projections, we need to handle deref


                        if let [ProjectionElem::Deref] = source_place.projection.as_slice() {
                            let target_local = source_place.local;
                            add_source(target_local);
                            add_child(target_local);
                        }
                    },
                    // Case: Use of mutable reference 

                    _ => {}
                }

                // Update the state for the destination local
                state.ancestry[dest_local] = new_ancestors;
                
            };

            // Perform revocation for the destination local
            if place.projection.first() == Some(&ProjectionElem::Deref) {
                // 2. Look up the type of the local variable (_1)
                let local_index = place.local;

                revoke_recursive(local_index, state);

                // // 3. Match on the Type Kind to distinguish them
                // match ty.kind() {
                //     // CASE A: Raw Pointer (*mut T or *const T)
                //     TyKind::RawPtr(_, Mutability::Mut) => {
                //         //println!("Write via RAW POINTER: {:?}", local_index);
                //         revoke_recursive(local_index, state);
                //     },

                //     // CASE B: Reference (&mut T or &T)
                //     TyKind::Ref(_, _, Mutability::Mut) => {
                //         //println!("Write via MUTABLE REFERENCE: {:?}", local_index);
                //         revoke_recursive(local_index, state);
                //     },

                //     _ => {}
                // }
            } 

        }
    }
}

impl Callbacks for MyAnalysisCallbacks {
    // This hook runs after parsing and analysis, but before code generation.
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &interface::Compiler,
        _tcx: TyCtxt<'tcx>
    ) -> Compilation {

        analyze_program(_tcx);

        // Stop compilation here (we don't want to output a binary)
        Compilation::Stop
    }
}

fn analyze_program<'tcx>(tcx: TyCtxt<'tcx>) {
    // Iterate over every function in the target crate
    for id in tcx.hir_crate_items(()).definitions() {
        let def_id = id.to_def_id();
        
        // Check if it's a function
        if tcx.def_kind(def_id).is_fn_like() {
            // Get the MIR (Mid-level Intermediate Representation)
            // "optimized_mir" is simpler to analyze than standard MIR
            let body = tcx.optimized_mir(def_id);
            
            println!("Analyzing function: {:?}", tcx.def_path_str(def_id));
            run_my_pointer_analysis(tcx, body);
        }
    }
}

fn run_my_pointer_analysis<'tcx>(tcx: TyCtxt<'tcx>, body: &Body<'tcx>) {
    // === YOUR RESEARCH LOGIC GOES HERE ===
    // Example: Print all local variable declarations
    // for (local, decl) in body.local_decls.iter_enumerated() {
    //     println!("  Local {:?}: {:?}", local, decl.ty);
    // }

    let analysis = AncestryAnalysis;
    let mut results = analysis.iterate_to_fixpoint(tcx, body, None).into_results_cursor(body);


    for (bb, block_data) in body.basic_blocks.iter_enumerated() {
        // Check statements in the block
        let mut statement_id = 0;

        for statement in &block_data.statements {
            let location = Location {
                block: bb,
                statement_index: statement_id,
           };
            statement_id += 1;

            if let StatementKind::Assign(box (place, rvalue)) = &statement.kind {
                // Case 1: Find raw pointer creation, e.g., `_2 = &raw mut _1;`
                //if let Rvalue::RawPtr(_, source_place) = rvalue {
                match rvalue { 
                    Rvalue::Ref(_, BorrowKind::Mut { .. }, source_place) => {
                    println!(
                        "  Found Mutable creation at {:?} in basic block {:?} :\n    {:?}",
                        statement.source_info.span, bb, statement
                    );

                    // Get the analysis state after this statement
                    results.seek_after_primary_effect(location);
                    let state_before = results.get();

                    if let Some(source_local) = source_place.as_local() {
                        let ancestors = &state_before.ancestry[source_local];
                        let children = &state_before.all_children[source_local];
                        println!("    Pointer is created from {:?}, which has ancestors: {:?}; and children {:?}", source_local, ancestors, children);
                    } else {
                        println!("    Pointer is created from a complex place: {:?}", source_place);
                    }

                    // new pointer local
                    if let Some(place_local) = place.as_local() {
                        let place_ancestors = &state_before.ancestry[place_local];
                        
                        println!("    The new pointer {:?} has ancestors: {:?}", place_local, place_ancestors);
                    } else {
                        println!("    The new pointer is a complex place: {:?}", place);
                    }

                    println!(); // for readability

                },
                    Rvalue::RawPtr(_, source_place) => {
                    println!(
                        "  Found raw pointer creation at {:?} in basic block {:?} :\n    {:?}",
                        statement.source_info.span, bb, statement
                    );

                    // Get the analysis state after this statement
                    results.seek_after_primary_effect(location);
                    let state_before = results.get();

                    if let [ProjectionElem::Deref] = source_place.projection.as_slice() {
                        // It is a deref! The local is:
                        let target_local = source_place.local;
                        // println!("We found a dereference of {:?}", target_local);
                        let ancestors = &state_before.ancestry[target_local];
                        let children = &state_before.all_children[target_local];
                        println!("    Pointer is created from {:?}, which has ancestors: {:?}, and children: {:?}", target_local, ancestors, children);
                    }

                    // new pointer local
                    if let Some(place_local) = place.as_local() {
                        let place_ancestors = &state_before.ancestry[place_local];
                        println!("    The new pointer {:?} has ancestors: {:?}", place_local, place_ancestors);
                    } else {
                        println!("    The new pointer is a complex place: {:?}", place);
                    }
                    
                    println!(); // for readability
                },
                    _ => {}
                };

                if place.projection.first() == Some(&ProjectionElem::Deref) {
                    
                    // 2. Look up the type of the local variable (_1)
                    let local_index = place.local;
                    let local_decl = &body.local_decls[local_index];
                    let ty = local_decl.ty;

                    // 3. Match on the Type Kind to distinguish them
                    match ty.kind() {
                        // CASE A: Raw Pointer (*mut T or *const T)
                        TyKind::RawPtr(ty, mutability) => {
                            println!("Write via RAW POINTER: {:?}", local_index);
                            if *mutability == Mutability::Mut {
                                println!("(It is a *mut T)");
                            }
                        },

                        // CASE B: Reference (&mut T or &T)
                        TyKind::Ref(_region, _inner_ty, mutability) => {
                            if *mutability == Mutability::Mut {
                                println!("Write via MUTABLE REFERENCE: {:?}", local_index);
                            } else {
                                // This is rare in valid Rust (writing to &T), 
                                // but possible via UnsafeCell.
                                println!("Write via SHARED REFERENCE: {:?}", local_index);
                            }
                        },

                        _ => println!("Other dereference (e.g., Box)"),
                    }

                    results.seek_before_primary_effect(location);
                    let results_before = results.get();
                    let ancestors = &results_before.ancestry[local_index];
                    let children = &results_before.all_children[local_index];
                    let revoked = &results_before.revoked;

                    println!("    Pointer is created from {:?}, which has ancestors: {:?}, children: {:?} and revoked: {:?}", local_index, ancestors, children, revoked);
                    if results_before.revoked.contains(local_index) {
                        println!("  ERROR: This write is to a REVOKED pointer: {:?}", local_index);
                    }
                }

            }

            // We simply print the statements here for demonstration
            //println!("  Statement in {:?} at BB{:?}: {:?}", statement.source_info.span, bb, statement);
        }
    }
}

fn main() {
    // Wrap the arguments. This makes your tool look like `rustc` to the system.
    let args: Vec<String> = std::env::args().collect();
    
    // Run the compiler with our custom callback
    run_compiler(&args, &mut MyAnalysisCallbacks);

        
}