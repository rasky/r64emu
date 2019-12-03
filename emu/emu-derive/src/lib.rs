#![recursion_limit = "128"]

#[macro_use]
extern crate synstructure;

#[macro_use]
extern crate quote;

extern crate proc_macro2;

use proc_macro2::{Ident, Span};
use std::num::ParseIntError;
use synstructure::BindStyle;
use synstructure::ToTokens;

decl_derive!([DeviceLE, attributes(reg, mem)] => derive_device_le);
decl_derive!([DeviceBE, attributes(reg, mem)] => derive_device_be);

// Copy of emu::bus::BusFill
#[derive(Debug, Clone, Copy)]
enum BusFill {
    None,
    Mirror,
    Fixed(u8),
}

impl Default for BusFill {
    fn default() -> Self {
        BusFill::None
    }
}

impl ToTokens for BusFill {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match *self {
            BusFill::None => quote_spanned!(Span::call_site() => BusFill::None).to_tokens(tokens),
            BusFill::Mirror => {
                quote_spanned!(Span::call_site() => BusFill::Mirror).to_tokens(tokens)
            }
            BusFill::Fixed(val) => {
                quote_spanned!(Span::call_site() => BusFill::Fixed(#val)).to_tokens(tokens)
            }
        }
    }
}

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
    wcb: bool,

    bank: usize,
    offset: u32,
    vsize: u32,
    fill: BusFill,
}

fn parse_u32(s: &str) -> Result<u32, ParseIntError> {
    if s.starts_with("0x") {
        u32::from_str_radix(&s[2..].replace("_", ""), 16)
    } else {
        s.parse::<u32>()
    }
}

fn parse_usize(s: &str) -> Result<usize, ParseIntError> {
    if s.starts_with("0x") {
        usize::from_str_radix(&s[2..].replace("_", ""), 16)
    } else {
        s.parse::<usize>()
    }
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
                ra.bank = kv[1]
                    .trim()
                    .parse::<usize>()
                    .expect(&format!("cannot parse bank: {:?}", kv[1].trim()));
            }
            "offset" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for offset", varname))
                }
                ra.offset = parse_u32(kv[1].trim())
                    .expect(&format!("cannot parse offset: {:?}", kv[1].trim()));
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

                ma.size = parse_usize(kv[1].trim())
                    .expect(&format!("cannot parse size: {:?}", kv[1].trim()));
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
            "wcb" => {
                if kv.len() != 1 {
                    panic!(format!("{}: unexpected argument for wcb", varname))
                }
                ma.wcb = true;
            }
            "bank" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for bank", varname))
                }
                ma.bank = parse_usize(kv[1].trim())
                    .expect(&format!("cannot parse bank: {:?}", kv[1].trim()));
            }
            "offset" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for offset", varname))
                }
                ma.offset = parse_u32(kv[1].trim())
                    .expect(&format!("cannot parse offset: {:?}", kv[1].trim()));
                offsetfound = true;
            }
            "vsize" => {
                if kv.len() != 2 {
                    panic!(format!("{}: no argument for size", varname))
                }

                ma.vsize = parse_u32(kv[1].trim())
                    .expect(&format!("cannot parse vsize: {:?}", kv[1].trim()));
            }
            "fill" => {
                let fillval = kv[1].replace(" ", "");
                let fillval = fillval.trim();

                ma.fill = match fillval {
                    "\"None\"" => BusFill::None,
                    "\"Mirror\"" => BusFill::Mirror,
                    _ => {
                        if fillval.starts_with("\"Fixed(") && fillval.ends_with(")\"") {
                            let fixed = parse_u32(&fillval[7..fillval.len() - 2]).expect(&format!(
                                "{}: cannot parse fixed-size variant value",
                                varname
                            ));
                            if fixed >= 256 {
                                panic!("{}: fixed-size value is too big", varname);
                            }
                            BusFill::Fixed(fixed as u8)
                        } else {
                            panic!("{}: {} is not a variant of bus::BusFill", varname, fillval);
                        }
                    }
                };
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
    if ma.readonly && ma.wcb {
        panic!(format!("{}: cannot specify wcb for readonly mem", varname));
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
    dev_ident: &Ident,
    dev_name: &str,
    varname: &str,
    ra: &RegAttributes,
) -> proc_macro2::TokenStream {
    let mut qrcb = quote! {None};
    let mut qwcb = quote! {None};

    if ra.wcb {
        let cbname = Ident::new(&format!("cb_write_{}", varname), Span::call_site());
        qwcb = quote! {
            Some(Rc::new(Box::new(move |old, val| {
                let dev = #dev_ident ::get_mut();
                dev. #cbname (old, val);
            })))
        };
    }

    if ra.rcb {
        let cbname = Ident::new(&format!("cb_read_{}", varname), Span::call_site());
        qrcb = quote! {
            Some(Rc::new(Box::new(move |val| {
                let dev = #dev_ident ::get_mut();
                let res = dev. #cbname (val);
                drop(dev);
                res
            })))
        }
    }

    let init = parse_u32(&ra.init).expect(&format!("cannot parse init value: {:?}", ra.init));
    let rwmask =
        parse_u32(&ra.rwmask).expect(&format!("cannot parse rwmask value: {:?}", ra.rwmask));
    let read = !ra.writeonly;
    let write = !ra.readonly;
    quote! {
        *#fi = Reg::new(
            concat!(#dev_name, "::", #varname),
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
    dev_ident: &Ident,
    structname: &str,
    varname: &str,
    ma: &MemAttributes,
) -> proc_macro2::TokenStream {
    let size = ma.size;
    if size == 0 {
        if ma.readonly {
            panic!("cannot set readonly for manually inited mem")
        }
        if ma.writeonly {
            panic!("cannot set writeonly for manually inited mem")
        }
        if ma.wcb {
            panic!("cannot set wcb for manually inited mem")
        }
        quote! {
            if #fi .len() == 0 {
                panic!("size not specified, and mem wasn't manually inited");
            }
        }
    } else {
        let read = !ma.readonly;
        let write = !ma.writeonly;
        let mut qwcb = quote! {None};

        if ma.wcb {
            let cbname = Ident::new(&format!("cb_write_{}", varname), Span::call_site());
            qwcb = quote! {
                Some(Rc::new(Box::new(move |addr, size, old, val| {
                    let dev = #dev_ident ::get_mut();
                    dev. #cbname (addr, size, old, val);
                })))
            };
        }
        quote! {
            if #fi .len() != 0 {
                panic!("don't specify size for already inited mem");
            }
            *#fi = Mem::new(concat!(#structname, "::", #varname), #size, MemFlags::new(#read, #write), #qwcb);
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
    quote! {
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
    let fill = ma.fill;
    quote! {
        if bank == #bank {
            bus.map_mem(base + #off, base + #off + #vsize - 1, &self. #varname, #fill)?;
        }
    }
}

fn derive_device(mut s: synstructure::Structure, bigendian: bool) -> proc_macro2::TokenStream {
    s.filter(|fi| fi.ast().attrs.len() != 0);
    s.bind_with(|_fi| BindStyle::RefMut);

    let dev_ident = s.ast().ident.clone();
    let dev_name = s.ast().ident.to_string();
    let mut dev_map = quote! {};
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
                dev_map = quote! {
                    #dev_map
                    #dm;
                };
                expand_reg_devinit(fi, &dev_ident, &dev_name, &varname, &ra)
            }

            "mem" => {
                let ma = parse_mem_attributes(&varname, &attrs[0].tts);

                let dm = expand_mem_devmap(fi, &varname, &ma);
                dev_map = quote! {
                    #dev_map
                    #dm;
                };
                expand_mem_devinit(fi, &dev_ident, &dev_name, &varname, &ma)
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
        use ::std::result::Result;
        use ::std::cell::{RefCell};
        use ::std::rc::{Rc};
        use ::std::pin::{Pin};
        use emu::bus::{CurrentDeviceMap, Bus, Device, BusFill};
        use byteorder:: #endian;

        #[allow(unused_imports)]
        use emu::bus::{Reg, RegFlags, Mem, MemFlags};

        gen impl Device for @Self {
            type Order = #endian;

            fn tag() -> &'static str {
                #dev_name
            }

            fn register(self: Box<Self>) {
                let mut pself = Pin::new(self);
                let pself_tag = #dev_name;
                match *pself {
                    #dev_init
                }
                CurrentDeviceMap().register(pself);
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
