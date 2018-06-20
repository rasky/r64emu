#[macro_use]
extern crate synstructure;

#[macro_use]
extern crate quote;

extern crate proc_macro2;

decl_derive!([RegBank, attributes(init, rwmask, rcb, wcb)] => regbank_derive);

fn regbank_derive(s: synstructure::Structure) -> proc_macro2::TokenStream {
    s.gen_impl(quote! {
        gen impl RegBank for @Self {
            fn init_regs(&mut self) {
                unimplemented!();
            }
        }
    })
}
