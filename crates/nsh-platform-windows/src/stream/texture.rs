//! Frames given as Direct3D 11 textures (`StreamHandle::push_d3d11_texture`),
//! on the pushing thread: which way a texture reaches the renderer
//! ([`route`]), and the two ways that need work here: a shared texture of
//! another device opened on the renderer's ([`Textures::open`], cached),
//! and the copy through the CPU when nothing else works ([`Textures::read`]).

use std::sync::Mutex;

use windows::{
    Win32::{
        Foundation::S_OK,
        Graphics::{
            Direct3D11::{
                D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_RESOURCE_MISC_SHARED,
                D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
                ID3D11Device, ID3D11Texture2D,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIKeyedMutex, IDXGIResource,
            },
        },
    },
    core::{IUnknown, Interface},
};

/// How long the copy through the CPU waits for a keyed mutex's key 0
/// (milliseconds): on the host's thread, never the UI thread's.
const KEY_WAIT_MS: u32 = 16;

/// The shared textures of other devices kept open on the renderer's.
const OPENED: usize = 8;

/// How a texture reaches the renderer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Route {
    /// The renderer's device, one picture a shader may read: drawn as it
    /// is, no copy.
    Direct,
    /// The renderer's device, but not drawable as it is (or guarded by a
    /// keyed mutex): copied on the GPU.
    Copy,
    /// Another device, shared (a legacy shared handle, perhaps a keyed
    /// mutex): opened on the renderer's and copied on the GPU.
    Shared,
    /// Another device, not shared: read back through the CPU.
    Cpu,
}

/// Why a texture cannot be a frame, if it cannot.
pub(crate) fn check(desc: &D3D11_TEXTURE2D_DESC) -> Result<(), &'static str> {
    if desc.Width == 0 || desc.Height == 0 {
        return Err("the size is empty");
    }
    if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
        return Err("the format is not DXGI_FORMAT_B8G8R8A8_UNORM");
    }
    if desc.SampleDesc.Count != 1 {
        return Err("the texture is multisampled");
    }
    Ok(())
}

/// How a texture of `desc` reaches the renderer: `same_device` when it is
/// of the renderer's device.
pub(crate) fn route(same_device: bool, desc: &D3D11_TEXTURE2D_DESC) -> Route {
    let drawable = desc.BindFlags & D3D11_BIND_SHADER_RESOURCE.0 as u32 != 0
        && desc.MipLevels == 1
        && desc.ArraySize == 1;
    let shared = desc.MiscFlags
        & (D3D11_RESOURCE_MISC_SHARED.0 | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0) as u32
        != 0;
    // A keyed texture is read only under its key, which the copy takes.
    match (same_device, drawable && !keyed(desc), shared) {
        (true, true, _) => Route::Direct,
        (true, false, _) => Route::Copy,
        (false, _, true) => Route::Shared,
        (false, _, false) => Route::Cpu,
    }
}

/// Whether a texture of `desc` is guarded by a keyed mutex.
pub(crate) fn keyed(desc: &D3D11_TEXTURE2D_DESC) -> bool {
    desc.MiscFlags & D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32 != 0
}

/// Whether `a` and `b` are the same COM object.
pub(crate) fn same_object(a: &impl Interface, b: &impl Interface) -> bool {
    match (a.cast::<IUnknown>(), b.cast::<IUnknown>()) {
        (Ok(a), Ok(b)) => a.as_raw() == b.as_raw(),
        _ => false,
    }
}

/// Takes key 0 of `mutex` within `ms` milliseconds; `false` when it did
/// not come (a timeout or an abandoned key are success codes `windows`
/// would not tell apart, hence the raw call).
pub(crate) fn acquire(mutex: &IDXGIKeyedMutex, ms: u32) -> bool {
    // SAFETY: a call on a live keyed mutex.
    unsafe { (Interface::vtable(mutex).AcquireSync)(mutex.as_raw(), 0, ms) == S_OK }
}

/// What the pushing side keeps between pushes.
#[derive(Default)]
struct Cache {
    /// The renderer's device the opened textures belong to.
    renderer: Option<ID3D11Device>,
    /// Host textures and their opened twins, the newest last.
    opened: Vec<(ID3D11Texture2D, ID3D11Texture2D)>,
    /// The CPU copy's staging texture: its device and size.
    staging: Option<(ID3D11Device, u32, u32, ID3D11Texture2D)>,
}

/// A stream handle's state for textures; any thread.
#[derive(Default)]
pub(crate) struct Textures {
    cache: Mutex<Cache>,
}

impl Textures {
    /// `texture` (of another device, shared) opened on `renderer`.
    pub(crate) fn open(
        &self,
        renderer: &ID3D11Device,
        texture: &ID3D11Texture2D,
    ) -> windows::core::Result<ID3D11Texture2D> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache
            .renderer
            .as_ref()
            .is_none_or(|r| !same_object(r, renderer))
        {
            cache.opened.clear();
            cache.renderer = Some(renderer.clone());
        }
        if let Some((_, opened)) = cache.opened.iter().find(|(t, _)| t == texture) {
            return Ok(opened.clone());
        }
        // SAFETY: COM calls on live objects; a legacy shared handle needs
        // no closing, and the device is free-threaded.
        let opened = unsafe {
            let handle = texture.cast::<IDXGIResource>()?.GetSharedHandle()?;
            let mut opened: Option<ID3D11Texture2D> = None;
            renderer.OpenSharedResource(handle, &mut opened)?;
            opened.ok_or_else(|| {
                windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL)
            })?
        };
        if cache.opened.len() >= OPENED {
            cache.opened.remove(0);
        }
        cache.opened.push((texture.clone(), opened.clone()));
        Ok(opened)
    }

    /// Reads `texture` (of `desc`) back through the CPU, on its own
    /// device's immediate context: `(stride, bytes)` of packed BGRA.
    pub(crate) fn read(
        &self,
        texture: &ID3D11Texture2D,
        desc: &D3D11_TEXTURE2D_DESC,
    ) -> windows::core::Result<(usize, Vec<u8>)> {
        let (width, height) = (desc.Width, desc.Height);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: COM calls on live objects with valid descriptions; the
        // mapped memory is read within `RowPitch * height` bytes and
        // unmapped before it is left.
        unsafe {
            let device = texture.GetDevice()?;
            let staging = match &cache.staging {
                Some((d, w, h, staging))
                    if same_object(d, &device) && (*w, *h) == (width, height) =>
                {
                    staging.clone()
                }
                _ => {
                    let staging_desc = D3D11_TEXTURE2D_DESC {
                        Width: width,
                        Height: height,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_STAGING,
                        BindFlags: 0,
                        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                        MiscFlags: 0,
                    };
                    let mut staging = None;
                    device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
                    let staging: ID3D11Texture2D = staging.ok_or_else(|| {
                        windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL)
                    })?;
                    cache.staging = Some((device.clone(), width, height, staging.clone()));
                    staging
                }
            };
            let context = device.GetImmediateContext()?;
            let mutex = if keyed(desc) {
                let mutex = texture.cast::<IDXGIKeyedMutex>()?;
                if !acquire(&mutex, KEY_WAIT_MS) {
                    return Err(windows::core::Error::from_hresult(
                        windows::Win32::Foundation::E_FAIL,
                    ));
                }
                Some(mutex)
            } else {
                None
            };
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, texture, 0, None);
            if let Some(mutex) = mutex {
                let _ = mutex.ReleaseSync(0);
            }
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let row = width as usize * 4;
            let pitch = mapped.RowPitch as usize;
            let mut bytes = Vec::with_capacity(row * height as usize);
            let base = mapped.pData.cast::<u8>().cast_const();
            for y in 0..height as usize {
                bytes.extend_from_slice(std::slice::from_raw_parts(base.add(y * pitch), row));
            }
            context.Unmap(&staging, 0);
            Ok((row, bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Graphics::{
        Direct3D11::{D3D11_BIND_RENDER_TARGET, D3D11_RESOURCE_MISC_SHARED_NTHANDLE},
        Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM},
    };

    use super::*;

    fn desc() -> D3D11_TEXTURE2D_DESC {
        D3D11_TEXTURE2D_DESC {
            Width: 64,
            Height: 32,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            ..Default::default()
        }
    }

    #[test]
    fn frames_are_bgra_pictures() {
        assert_eq!(check(&desc()), Ok(()));
        for format in [DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB] {
            assert!(
                check(&D3D11_TEXTURE2D_DESC {
                    Format: format,
                    ..desc()
                })
                .is_err()
            );
        }
        assert!(check(&D3D11_TEXTURE2D_DESC { Width: 0, ..desc() }).is_err());
        let samples = DXGI_SAMPLE_DESC {
            Count: 4,
            Quality: 0,
        };
        assert!(
            check(&D3D11_TEXTURE2D_DESC {
                SampleDesc: samples,
                ..desc()
            })
            .is_err()
        );
    }

    #[test]
    fn textures_take_the_cheapest_way() {
        // The renderer's device: drawn as it is, or copied on the GPU when
        // a shader cannot read it.
        assert_eq!(route(true, &desc()), Route::Direct);
        assert_eq!(
            route(
                true,
                &D3D11_TEXTURE2D_DESC {
                    BindFlags: 0,
                    ..desc()
                }
            ),
            Route::Copy
        );
        assert_eq!(
            route(
                true,
                &D3D11_TEXTURE2D_DESC {
                    MipLevels: 2,
                    ..desc()
                }
            ),
            Route::Copy
        );
        // Another device: opened when shared, else read back.
        let shared = D3D11_RESOURCE_MISC_SHARED.0 as u32;
        let keyed_flag = D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32;
        assert_eq!(
            route(
                false,
                &D3D11_TEXTURE2D_DESC {
                    MiscFlags: shared,
                    ..desc()
                }
            ),
            Route::Shared
        );
        let keyed_desc = D3D11_TEXTURE2D_DESC {
            MiscFlags: keyed_flag,
            ..desc()
        };
        assert_eq!(route(false, &keyed_desc), Route::Shared);
        assert!(keyed(&keyed_desc));
        // On the renderer's device too: copied under its key, never drawn
        // as it is.
        assert_eq!(route(true, &keyed_desc), Route::Copy);
        assert_eq!(route(false, &desc()), Route::Cpu);
        // Only NT handles: read back (legacy handles are opened).
        let nt = D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 as u32;
        assert_eq!(
            route(
                false,
                &D3D11_TEXTURE2D_DESC {
                    MiscFlags: nt,
                    ..desc()
                }
            ),
            Route::Cpu
        );
    }
}
