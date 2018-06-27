#![recursion_limit = "128"]
#[macro_use]
extern crate synstructure;

#[macro_use]
extern crate quote;

extern crate proc_macro2;

use proc_macro2::{Ident, Span};
use synstructure::BindStyle;

decl_derive!([DeviceLE, attributes(reg, mem)] => derive_device_le);
decl_derive!([DeviceBE, attributes(reg, mem)] => derive_device_be);

#[derive(Default, Debug)]
struct RegAttributes {
    rwmask: String,
    init: String,
    wcb: bool,
    rcb: bool,
    readonly: bool,
    writeonly: bool,

    bank: usize,
    offset: u32,
}

#[derive(Default, Debug)]
struct MemAttributes {
    size: usize,
    readonly: bool,
    writeonly: bool,

    bank: usize,
    offset: u32,
    vsize: u32,
}

fn parse_reg_attributes(varname: &str, attrs: &proc_macro2::TokenStream) -> RegAttributes {
    let mut ra = RegAttributes::default();
    let mut offsetfound = false;

    let allattrs = format!("{}", attrs);
    for attr in allattrs[1..allattrs.len() - 1].split(",") {
        let kv = attr.split("=").collect::<Vec<_>>();
        match kv[0].trim().as_ref() {
            "rwmask" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for rwmask", varname))
                }
                ra.rwmask = kv[1].trim().to_string();
            }
            "init" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for init", varname))
                }
                ra.init = kv[1].trim().to_string();
            }
            "wcb" => {
                if kv.len() != 1 {
                    panic!(format!("{}: unexpected argument for wcb", varname))
                }
                ra.wcb = true;
            }
            "rcb" => {
                if kv.len() != 1 {
                    panic!(format!("{}: unexpected argument for rcb", varname))
                }
                ra.rcb = true;
            }
            "readonly" => {
                if kv.len() != 1 {
                    panic!(format!("{}: unexpected argument for readonly", varname))
                }
                ra.readonly = true;
            }
            "writeonly" => {
                if kv.len() != 1 {
                    panic!(format!("{}: unexpected argument for writeonly", varname))
                }
                ra.writeonly = true;
            }
            "bank" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for bank", varname))
                }
                ra.bank = kv[1].trim().parse::<usize>().unwrap();
            }
            "offset" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for offset", varname))
                }
                ra.offset = kv[1].trim().parse::<u32>().unwrap();
                offsetfound = true;
            }
            _ => panic!(format!("{}: invalid attribute: {}", varname, kv[0].trim())),
        }
    }
    if ra.readonly && ra.writeonly {
        panic!(format!(
            "{}: cannot be both readonly and writeonly",
            varname
        ));
    }
    if ra.readonly && ra.wcb {
        panic!(format!("{}: cannot specify wcb for readonly reg", varname));
    }
    if ra.writeonly && ra.rcb {
        panic!(format!("{}: cannot specify rcb for writeonly reg", varname));
    }
    if !offsetfound {
        panic!(format!("{}: mandatory offset is missing", varname));
    }
    if ra.init.is_empty() {
        ra.init = String::from("0");
    }
    if ra.rwmask.is_empty() {
        ra.rwmask = String::from("4294967295");
    }
    return ra;
}

fn parse_mem_attributes(varname: &str, attrs: &proc_macro2::TokenStream) -> MemAttributes {
    let mut ma = MemAttributes::default();
    let mut offsetfound = false;

    let allattrs = format!("{}", attrs);
    for attr in allattrs[1..allattrs.len() - 1].split(",") {
        let kv = attr.split("=").collect::<Vec<_>>();
        match kv[0].trim().as_ref() {
            "size" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for size", varname))
                }

                ma.size = kv[1].trim().parse::<usize>().unwrap();
            }
            "readonly" => {
                if kv.len() != 1 {
                    panic!(format!("{}: unexpected argument for readonly", varname))
                }
                ma.readonly = true;
            }
            "writeonly" => {
                if kv.len() != 1 {
                    panic!(format!("{}: unexpected argument for writeonly", varname))
                }
                ma.writeonly = true;
            }
            "bank" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for bank", varname))
                }
                ma.bank = kv[1].trim().parse::<usize>().unwrap();
            }
            "offset" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for offset", varname))
                }
                ma.offset = kv[1].trim().parse::<u32>().unwrap();
                offsetfound = true;
            }
            "vsize" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for size", varname))
                }

                ma.vsize = kv[1].trim().parse::<u32>().unwrap();
            }
            _ => panic!(format!("{}: invalid attribute: {}", varname, kv[0].trim())),
        }
    }

    if !offsetfound {
        panic!(format!("{}: mandatory offset is missing", varname));
    }
    if ma.vsize == 0 {
        ma.vsize = ma.size as u32;
    }
    if ma.readonly && ma.writeonly {
        panic!(format!(
            "{}: cannot be both readonly and writeonly",
            varname
        ));
    }
    return ma;
}

fn expand_reg_devinit(
    fi: &synstructure::BindingInfo,
    varname: &str,
    ra: &RegAttributes,
) -> proc_macro2::TokenStream {
    let mut qrcb = quote!{None};
    let mut qwcb = quote!{None};
    let mut initbody = quote!{};

    if ra.wcb {
        initbody = quote!{
            #initbody
            let wdevw = Rc::downgrade(&wself);
        };
        let cbname = Ident::new(&format!("cb_write_{}", varname), Span::call_site());
        qwcb = quote!{
            Some(Rc::new(Box::new(move |old, val| {
                let dev = wdevw.upgrade().unwrap();
                dev.borrow_mut(). #cbname (old, val);
            })))
        };
    }

    if ra.rcb {
        initbody = quote!{
            #initbody
            let wdevr = Rc::downgrade(&wself);
        };
        let cbname = Ident::new(&format!("cb_read_{}", varname), Span::call_site());
        qrcb = quote!{
            Some(Rc::new(Box::new(move |val| {
                let dev = wdevr.upgrade().unwrap();
                let res = dev.borrow(). #cbname (val);
                drop(dev);
                res
            })))
        }
    }

    let init = ra.init.parse::<u32>().unwrap();
    let rwmask = ra.rwmask.parse::<u32>().unwrap();
    let read = !ra.writeonly;
    let write = !ra.readonly;
    quote! {
        #initbody
        *#fi = Reg::new(
            #varname,
            #init,
            #rwmask,
            RegFlags::new(#read, #write),
            #qwcb,
            #qrcb,
        );
    }
}

fn expand_mem_devinit(
    fi: &synstructure::BindingInfo,
    _varname: &str,
    ma: &MemAttributes,
) -> proc_macro2::TokenStream {
    let size = ma.size;
    let read = !ma.readonly;
    let write = !ma.writeonly;
    if size == 0 {
        if ma.readonly {
            panic!("cannot set readonly for manully inited mem")
        }
        if ma.writeonly {
            panic!("cannot set writeonly for manully inited mem")
        }
        quote!{
            if #fi .len() == 0 {
                panic!("size not specified, and mem wasn't manually inited");
            }
        }
    } else {
        quote!{
            if #fi .len() != 0 {
                panic!("don't specify size for already inited mem");
            }
            *#fi = Mem::new(#size, MemFlags::new(#read, #write));
        }
    }
}

fn expand_reg_devmap(
    _fi: &synstructure::BindingInfo,
    varname: &str,
    ra: &RegAttributes,
) -> proc_macro2::TokenStream {
    let bank = ra.bank;
    let off = ra.offset;
    let varname = Ident::new(varname, Span::call_site());
    quote!{
        if bank == #bank {
            bus.map_reg(base + #off, &self. #varname)?;
        }
    }
}

fn expand_mem_devmap(
    _fi: &synstructure::BindingInfo,
    varname: &str,
    ma: &MemAttributes,
) -> proc_macro2::TokenStream {
    let bank = ma.bank;
    let off = ma.offset;
    let vsize = ma.vsize;
    let varname = Ident::new(varname, Span::call_site());
    quote!{
        if bank == #bank {
            bus.map_mem(base + #off, base + #off + #vsize - 1, &self. #varname)?;
        }
    }
}

fn derive_device(mut s: synstructure::Structure, bigendian: bool) -> proc_macro2::TokenStream {
    s.filter(|fi| fi.ast().attrs.len() != 0);
    s.bind_with(|_fi| BindStyle::RefMut);

    let mut dev_map = quote!{};
    let dev_init = s.each(|fi| {
        let varname = fi.ast().ident.as_ref().unwrap().to_string();

        let attrs = &fi.ast().attrs;
        if attrs.len() != 1 {
            panic!(format!("{}: too many attributes", varname));
        }

        match attrs[0]
            .path
            .segments
            .last()
            .unwrap()
            .value()
            .ident
            .to_string()
            .as_ref()
        {
            "reg" => {
                let ra = parse_reg_attributes(&varname, &attrs[0].tts);

                let dm = expand_reg_devmap(fi, &varname, &ra);
                dev_map = quote!{
                    #dev_map
                    #dm;
                };
                expand_reg_devinit(fi, &varname, &ra)
            }

            "mem" => {
                let ma = parse_mem_attributes(&varname, &attrs[0].tts);

                let dm = expand_mem_devmap(fi, &varname, &ma);
                dev_map = quote!{
                    #dev_map
                    #dm;
                };
                expand_mem_devinit(fi, &varname, &ma)
            }
            _ => unreachable!(),
        }
    });

    let endian = Ident::new(
        if bigendian {
            "BigEndian"
        } else {
            "LittleEndian"
        },
        Span::call_site(),
    );
    s.gen_impl(quote! {
        extern crate emu;
        extern crate byteorder;
        use ::std::result::Result;
        use ::std::cell::{RefCell};
        use ::std::rc::{Rc};
        use emu::bus::{Bus, Device, Reg, RegFlags, Mem, MemFlags};
        use byteorder:: #endian;

        gen impl Device for @Self {
            type Order = #endian;

            fn dev_init(&mut self, wself: Rc<RefCell<Self>>) {
                match *self {
                    #dev_init
                }
            }

            fn dev_map(&self, bus: &mut Bus<Self::Order>, bank: usize, base: u32,) -> Result<(), &'static str> {
                #dev_map
                Ok(())
            }
        }
    })
}

fn derive_device_le(s: synstructure::Structure) -> proc_macro2::TokenStream {
    derive_device(s, false)
}

fn derive_device_be(s: synstructure::Structure) -> proc_macro2::TokenStream {
    derive_device(s, true)
}

#[cfg(test)]
mod tests {}
