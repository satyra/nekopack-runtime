//! Native Trans mask surface and byte kernels. See System_Transition_Mask.md.
use crate::dat::DatGame;
use nekopack_image::{decode_png, RgbaImage};
use std::{error::Error, io::{Read, Seek}, rc::Rc};

const BUDGET: usize = 64 * 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Default)]
pub(crate) struct MaskState {
    latest: Option<Rc<RgbaImage>>,
}

impl MaskState {
    pub(crate) fn is_pristine(&self) -> bool {self.latest.is_none()}

    pub(crate) fn load<R: Read + Seek>(
        &mut self, store: &mut DatGame<R>, name: &str, size: [u32; 2],
    ) -> Result<Rc<RgbaImage>> {
        // The native loader tries PNG before JPEG. Do not mistake an existing,
        // unsupported JPEG or a damaged PNG for the missing-resource branch.
        let source = match store.read_optional("image/mask", &format!("{name}.png"))? {
            Some(bytes) => Some(decode_png(&bytes, BUDGET)?),
            None => {
                if store.has_resource("image/mask", &format!("{name}.jpg"))? {
                    return Err("original transition JPEG masks are not implemented".into());
                }
                None
            }
        };
        self.apply(source.as_ref(), size)
    }

    fn apply(&mut self, source: Option<&RgbaImage>, size: [u32; 2]) -> Result<Rc<RgbaImage>> {
        if let Some(previous) = &self.latest {
            if [previous.width(), previous.height()] != size {
                return Err("original transition mask surface dimensions changed".into());
            }
            if source.is_none() {return Ok(Rc::clone(previous));}
        }
        let mut surface = match &self.latest {
            Some(previous) => RgbaImage::from_rgba(size[0], size[1], previous.pixels().to_vec(), BUDGET)?,
            None => RgbaImage::filled(size[0], size[1], [0; 4], BUDGET)?,
        };
        if let Some(source) = source {
            // 0x405410 explicitly blits at (0,0), ignoring embedded offsets.
            // Source dimensions are clipped, not scaled to the screen.
            for y in 0..size[1].min(source.height()) as usize {
                for x in 0..size[0].min(source.width()) as usize {
                    let src = &source.pixels()[(y * source.width() as usize + x) * 4..][..4];
                    let dst = &mut surface.pixels_mut()[(y * size[0] as usize + x) * 4..][..4];
                    if src[3] == 255 {dst.copy_from_slice(src);}
                    else {
                        for (old, next) in dst.iter_mut().zip(src) {
                            *old = interpolate(*old, *next, i32::from(src[3]));
                        }
                    }
                }
            }
        }
        let surface = Rc::new(surface);
        self.latest = Some(Rc::clone(&surface));
        Ok(surface)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Mode { Hard, Fade }
impl Mode {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "2" => Ok(Self::Hard),
            "3" => Ok(Self::Fade),
            _ => Err("invalid original transition mask mode".into()),
        }
    }
}

fn interpolate(old: u8, next: u8, alpha: i32) -> u8 {
    (i32::from(old) + ((i32::from(next) - i32::from(old)) * alpha).div_euclid(256)) as u8
}

pub(crate) fn render(begin: &RgbaImage, end: &RgbaImage, mask: &RgbaImage, p: i32, mode: Mode) -> Result<RgbaImage> {
    let size = [begin.width(), begin.height()];
    if [end.width(), end.height()] != size || [mask.width(), mask.height()] != size {
        return Err("original transition snapshot/mask dimensions differ".into());
    }
    let pixels = begin.pixels().iter().zip(end.pixels()).zip(mask.pixels()).map(|((&old, &next), &threshold)| {
        if p <= 0 {old}
        else if p >= 255 {next}
        else {match mode {
            Mode::Hard => if i32::from(threshold) + p >= 255 {next} else {old},
            Mode::Fade => interpolate(old, next, (i32::from(threshold) + 2 * p - 255).clamp(0, 255)),
        }}
    }).collect();
    Ok(RgbaImage::from_rgba(size[0], size[1], pixels, BUDGET)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(pixels: &[u8]) -> RgbaImage {
        RgbaImage::from_rgba((pixels.len()/4) as u32, 1, pixels.to_vec(), BUDGET).unwrap()
    }
    #[test]
    fn native_byte_channels_threshold_and_signed_rounding() {
        let a=image(&[200,20,100,255]);
        let b=image(&[20,200,200,0]);
        let m=image(&[0,255,127,128]);
        assert_eq!(render(&a,&b,&m,128,Mode::Hard).unwrap().pixels(), &[200,200,200,0]);
        assert_eq!(render(&a,&b,&m,128,Mode::Fade).unwrap().pixels(), &[199,199,150,126]);
        for mode in [Mode::Hard, Mode::Fade] {
            for p in [i32::MIN,-1,0] {assert_eq!(render(&a,&b,&m,p,mode).unwrap(),a);}
            for p in [255,256,i32::MAX] {assert_eq!(render(&a,&b,&m,p,mode).unwrap(),b);}
        }
    }
    #[test]
    fn missing_alpha_clipping_and_old_snapshots() {
        let mut state=MaskState::default();
        let initial=state.apply(None,[2,1]).unwrap();
        assert_eq!(initial.pixels(), &[0;8]);
        let first=state.apply(Some(&image(&[200,100,50,255,50,100,200,255])),[2,1]).unwrap();
        let second=state.apply(Some(&image(&[20,200,100,128])),[2,1]).unwrap();
        assert_eq!(second.pixels(), &[110,150,75,191,50,100,200,255]);
        assert_eq!(first.pixels(), &[200,100,50,255,50,100,200,255]);
        assert_eq!(initial.pixels(), &[0;8]);
        assert!(Rc::ptr_eq(&state.apply(None,[2,1]).unwrap(),&second));
        assert_eq!(state.apply(Some(&image(&[0;12])),[2,1]).unwrap().pixels(),second.pixels());
        assert!(state.apply(None,[3,1]).is_err());
    }
    #[test]
    fn soft_kernel_matches_mmx_modular_arithmetic() {
        for old in 0u16..=255 {
            for next in [0u16,1,127,128,254,255] {
                for alpha in 0u16..=255 {
                    let product=next.wrapping_sub(old).wrapping_mul(alpha);
                    let mmx=(old as u8).wrapping_add((product>>8) as u8);
                    assert_eq!(interpolate(old as u8,next as u8,i32::from(alpha)),mmx);
                }
            }
        }
    }
    #[test]
    fn native_registration_mask_modes_and_snapshot_ids() {
        let mut vm=nekopack_script::system::SystemVm::new();
        vm.eval("headless",include_str!("../../../nekopack-script/host/headless.nut")).unwrap();
        vm.eval("host",include_str!("../../host/system_command.nut")).unwrap();
        vm.eval("mask-contract",r#"
            Config <- {Effect=1}
            SystemCommandTransBegin();SystemCommandTransEnd()
            SystemCommandTransSet(12.9,"","fade")
            if(SystemCommandHost.trans_mode!=1 || SystemCommandHost.trans_duration!=12)throw "empty mask mode"
            SystemCommandTransSet(100,"SYNTHETIC","FaDe")
            if(SystemCommandHost.trans_mode!=3 || SystemCommandHost.trans_mask_id!=1)throw "fade mode"
            SystemCommandRenderTrans(128)
            local captured=clone SystemCommandHost.current
            SystemCommandTransSet(200,"SECOND","other")
            if(SystemCommandHost.trans_mode!=2 || SystemCommandHost.trans_mask_id!=2)throw "hard mode"
            if(captured[0][3]!=1)throw "mask snapshot ID changed"
            Config.Effect=0
            SystemCommandTransSet(300,"THIRD")
            if(SystemCommandHost.trans_duration!=0 || SystemCommandHost.trans_loads.len()!=3)throw "disabled effect lost mask load"
            SystemCommandTransSet(100)
            if(SystemCommandHost.trans_mode!=1 || SystemCommandHost.trans_mask_id!=3)throw "plain transition cleared mask"
            local rejected=false
            try{SystemCommandTransSet(10,null)}catch(error){rejected=true}
            if(!rejected)throw "native string typemask"
            return SystemCommandLines(captured)
        "#).unwrap();
        assert_eq!(vm.result(),"Y\t128\t3\t1\t510a\t510a\n");
    }
}
