//! Arrastar um arquivo daqui para outro programa (OLE drag & drop).
//!
//! É a única parte do app que monta **vtables COM à mão**. O `windows-sys`
//! declara as interfaces como `*mut c_void` — ele é o binding cru, e quem
//! implementa um objeto COM monta a tabela de funções. A alternativa seria a
//! crate `windows` com a macro `implement`, que está atrás da feature `ocr`;
//! acoplar arrastar-e-soltar ao reconhecimento de texto seria pior.
//!
//! O que se arrasta é um **arquivo** (`CF_HDROP`), e não a imagem em memória.
//! É o formato que o Explorer, o Word, o Slack e as caixas de anexo aceitam;
//! oferecer bitmap serviria a menos programas e ainda exigiria renderização
//! atrasada. O PNG é gravado antes, num arquivo temporário.
//!
//! ## Tempo de vida
//!
//! `DoDragDrop` é **síncrono**: ele só volta quando o usuário solta o botão.
//! Os dois objetos vivem na pilha desta função durante a chamada inteira, e
//! por isso `Release` aqui não libera nada — ele só decrementa. Isso não é
//! preguiça: é o que elimina de vez a classe de erro mais perigosa deste
//! código, a liberação dupla, num objeto cujo tempo de vida já é conhecido e
//! garantido por construção.

#[cfg(windows)]
pub use imp::drag_file;

#[cfg(not(windows))]
pub fn drag_file(_path: &std::path::Path) -> bool {
    false
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::path::Path;

    use windows_sys::core::{GUID, HRESULT};
    use windows_sys::Win32::Foundation::{
        DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_FORMATETC, E_FAIL,
        E_NOINTERFACE, E_NOTIMPL, HGLOBAL, S_FALSE, S_OK,
    };
    use windows_sys::Win32::System::Com::{FORMATETC, STGMEDIUM, TYMED_HGLOBAL};
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::System::Ole::{
        DoDragDrop, OleInitialize, OleUninitialize, DROPEFFECT_COPY,
    };
    use windows_sys::Win32::UI::Shell::SHCreateStdEnumFmtEtc;

    /// `CF_HDROP`, o formato de "uma lista de arquivos".
    const CF_HDROP: u16 = 15;
    /// Botões do mouse em `grfKeyState`, para saber se o arrasto acabou.
    const MK_LBUTTON: u32 = 0x0001;
    const MK_RBUTTON: u32 = 0x0002;
    /// `DVASPECT_CONTENT`.
    const DVASPECT_CONTENT: u32 = 1;

    const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
    const IID_IDATAOBJECT: GUID = GUID::from_u128(0x0000010e_0000_0000_c000_000000000046);
    const IID_IDROPSOURCE: GUID = GUID::from_u128(0x00000121_0000_0000_c000_000000000046);

    fn same_iid(a: *const GUID, b: &GUID) -> bool {
        // SAFETY: o chamador de QueryInterface sempre passa um IID válido.
        let a = unsafe { &*a };
        a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
    }

    // -----------------------------------------------------------------------
    // IDataObject
    // -----------------------------------------------------------------------

    #[repr(C)]
    struct DataObjectVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
        get_data_here:
            unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
        query_get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC) -> HRESULT,
        get_canonical_format_etc:
            unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut FORMATETC) -> HRESULT,
        set_data: unsafe extern "system" fn(
            *mut c_void,
            *const FORMATETC,
            *const STGMEDIUM,
            i32,
        ) -> HRESULT,
        enum_format_etc: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT,
        d_advise: unsafe extern "system" fn(
            *mut c_void,
            *const FORMATETC,
            u32,
            *mut c_void,
            *mut u32,
        ) -> HRESULT,
        d_unadvise: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
        enum_d_advise: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    }

    /// O objeto: a vtable **primeiro**, porque é isso que um ponteiro de
    /// interface COM aponta.
    #[repr(C)]
    struct DataObject {
        vtbl: *const DataObjectVtbl,
        refs: u32,
        /// O `DROPFILES` já montado, pronto para ser copiado num HGLOBAL.
        dropfiles: Vec<u8>,
    }

    unsafe extern "system" fn do_query_interface(
        this: *mut c_void,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> HRESULT {
        if out.is_null() {
            return E_FAIL;
        }
        unsafe {
            if same_iid(iid, &IID_IUNKNOWN) || same_iid(iid, &IID_IDATAOBJECT) {
                *out = this;
                do_add_ref(this);
                return S_OK;
            }
            *out = std::ptr::null_mut();
        }
        E_NOINTERFACE
    }

    unsafe extern "system" fn do_add_ref(this: *mut c_void) -> u32 {
        // SAFETY: `this` é sempre o `DataObject` que criamos.
        let obj = unsafe { &mut *(this as *mut DataObject) };
        obj.refs += 1;
        obj.refs
    }

    /// Não libera: ver a nota de tempo de vida no topo do módulo.
    unsafe extern "system" fn do_release(this: *mut c_void) -> u32 {
        let obj = unsafe { &mut *(this as *mut DataObject) };
        obj.refs = obj.refs.saturating_sub(1);
        obj.refs
    }

    /// Este formato serve? Só `CF_HDROP` em HGLOBAL.
    fn aceita(fmt: *const FORMATETC) -> bool {
        if fmt.is_null() {
            return false;
        }
        // SAFETY: ponteiro do chamador COM, não nulo.
        let fmt = unsafe { &*fmt };
        fmt.cfFormat == CF_HDROP && (fmt.tymed & TYMED_HGLOBAL as u32) != 0
    }

    unsafe extern "system" fn do_get_data(
        this: *mut c_void,
        fmt: *const FORMATETC,
        medium: *mut STGMEDIUM,
    ) -> HRESULT {
        if !aceita(fmt) || medium.is_null() {
            return DV_E_FORMATETC;
        }
        let obj = unsafe { &*(this as *const DataObject) };
        // SAFETY: o HGLOBAL passa a ser do chamador — é o contrato do
        // `GetData` com `pUnkForRelease` nulo.
        unsafe {
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, obj.dropfiles.len());
            if hglobal.is_null() {
                return E_FAIL;
            }
            let dst = GlobalLock(hglobal);
            if dst.is_null() {
                return E_FAIL;
            }
            std::ptr::copy_nonoverlapping(obj.dropfiles.as_ptr(), dst as *mut u8, obj.dropfiles.len());
            GlobalUnlock(hglobal);

            let out = &mut *medium;
            out.tymed = TYMED_HGLOBAL as u32;
            out.u.hGlobal = hglobal as HGLOBAL;
            out.pUnkForRelease = std::ptr::null_mut();
        }
        S_OK
    }

    unsafe extern "system" fn do_get_data_here(
        _this: *mut c_void,
        _fmt: *const FORMATETC,
        _medium: *mut STGMEDIUM,
    ) -> HRESULT {
        // Só faz sentido para meios que o chamador aloca; o nosso é HGLOBAL.
        E_NOTIMPL
    }

    unsafe extern "system" fn do_query_get_data(
        _this: *mut c_void,
        fmt: *const FORMATETC,
    ) -> HRESULT {
        if aceita(fmt) {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    unsafe extern "system" fn do_get_canonical(
        _this: *mut c_void,
        _fmt: *const FORMATETC,
        out: *mut FORMATETC,
    ) -> HRESULT {
        if !out.is_null() {
            // SAFETY: ponteiro de saída do chamador.
            unsafe { (*out).ptd = std::ptr::null_mut() };
        }
        // "Não há forma canônica diferente da pedida."
        S_FALSE
    }

    unsafe extern "system" fn do_set_data(
        _this: *mut c_void,
        _fmt: *const FORMATETC,
        _medium: *const STGMEDIUM,
        _release: i32,
    ) -> HRESULT {
        // Somos fonte, não destino: ninguém escreve neste objeto.
        E_NOTIMPL
    }

    /// O enumerador vem pronto do shell: implementar um segundo objeto COM
    /// para listar um formato só seria trabalho sem retorno.
    unsafe extern "system" fn do_enum_format_etc(
        _this: *mut c_void,
        direction: u32,
        out: *mut *mut c_void,
    ) -> HRESULT {
        const DATADIR_GET: u32 = 1;
        if direction != DATADIR_GET || out.is_null() {
            return E_NOTIMPL;
        }
        let fmt = FORMATETC {
            cfFormat: CF_HDROP,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL as u32,
        };
        // SAFETY: um formato, ponteiro de saída válido conferido acima.
        unsafe { SHCreateStdEnumFmtEtc(1, &fmt, out) }
    }

    unsafe extern "system" fn do_advise(
        _this: *mut c_void,
        _fmt: *const FORMATETC,
        _flags: u32,
        _sink: *mut c_void,
        _conn: *mut u32,
    ) -> HRESULT {
        // OLE_E_ADVISENOTSUPPORTED
        0x8004_0003_u32 as HRESULT
    }

    unsafe extern "system" fn do_unadvise(_this: *mut c_void, _conn: u32) -> HRESULT {
        0x8004_0003_u32 as HRESULT
    }

    unsafe extern "system" fn do_enum_advise(
        _this: *mut c_void,
        _out: *mut *mut c_void,
    ) -> HRESULT {
        0x8004_0003_u32 as HRESULT
    }

    static DATA_VTBL: DataObjectVtbl = DataObjectVtbl {
        query_interface: do_query_interface,
        add_ref: do_add_ref,
        release: do_release,
        get_data: do_get_data,
        get_data_here: do_get_data_here,
        query_get_data: do_query_get_data,
        get_canonical_format_etc: do_get_canonical,
        set_data: do_set_data,
        enum_format_etc: do_enum_format_etc,
        d_advise: do_advise,
        d_unadvise: do_unadvise,
        enum_d_advise: do_enum_advise,
    };

    // -----------------------------------------------------------------------
    // IDropSource
    // -----------------------------------------------------------------------

    #[repr(C)]
    struct DropSourceVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        query_continue_drag: unsafe extern "system" fn(*mut c_void, i32, u32) -> HRESULT,
        give_feedback: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    }

    #[repr(C)]
    struct DropSource {
        vtbl: *const DropSourceVtbl,
        refs: u32,
    }

    unsafe extern "system" fn ds_query_interface(
        this: *mut c_void,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> HRESULT {
        if out.is_null() {
            return E_FAIL;
        }
        unsafe {
            if same_iid(iid, &IID_IUNKNOWN) || same_iid(iid, &IID_IDROPSOURCE) {
                *out = this;
                ds_add_ref(this);
                return S_OK;
            }
            *out = std::ptr::null_mut();
        }
        E_NOINTERFACE
    }

    unsafe extern "system" fn ds_add_ref(this: *mut c_void) -> u32 {
        let obj = unsafe { &mut *(this as *mut DropSource) };
        obj.refs += 1;
        obj.refs
    }

    unsafe extern "system" fn ds_release(this: *mut c_void) -> u32 {
        let obj = unsafe { &mut *(this as *mut DropSource) };
        obj.refs = obj.refs.saturating_sub(1);
        obj.refs
    }

    /// Esc cancela; soltar o botão esquerdo solta o arquivo.
    unsafe extern "system" fn ds_query_continue_drag(
        _this: *mut c_void,
        escape: i32,
        key_state: u32,
    ) -> HRESULT {
        if escape != 0 || (key_state & MK_RBUTTON) != 0 {
            return DRAGDROP_S_CANCEL;
        }
        if (key_state & MK_LBUTTON) == 0 {
            return DRAGDROP_S_DROP;
        }
        S_OK
    }

    /// Cursores padrão do OLE: desenhar os nossos só faria o arrasto parecer
    /// diferente do resto do sistema.
    unsafe extern "system" fn ds_give_feedback(_this: *mut c_void, _effect: u32) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }

    static DROP_VTBL: DropSourceVtbl = DropSourceVtbl {
        query_interface: ds_query_interface,
        add_ref: ds_add_ref,
        release: ds_release,
        query_continue_drag: ds_query_continue_drag,
        give_feedback: ds_give_feedback,
    };

    // -----------------------------------------------------------------------

    /// Monta o bloco `DROPFILES` seguido do caminho em UTF-16, terminado por
    /// **dois** nulos — é assim que o formato marca o fim da lista.
    fn dropfiles_blob(path: &Path) -> Vec<u8> {
        // DROPFILES: pFiles(u32), pt(POINTL), fNC(i32), fWide(i32) = 20 bytes.
        const CABECALHO: u32 = 20;
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);
        wide.push(0);

        let mut blob = Vec::with_capacity(CABECALHO as usize + wide.len() * 2);
        blob.extend_from_slice(&CABECALHO.to_le_bytes());
        blob.extend_from_slice(&0i32.to_le_bytes()); // pt.x
        blob.extend_from_slice(&0i32.to_le_bytes()); // pt.y
        blob.extend_from_slice(&0i32.to_le_bytes()); // fNC
        blob.extend_from_slice(&1i32.to_le_bytes()); // fWide = UTF-16
        for unidade in wide {
            blob.extend_from_slice(&unidade.to_le_bytes());
        }
        blob
    }

    use std::os::windows::ffi::OsStrExt as _;

    /// Arrasta `path` como se fosse um arquivo do Explorer.
    ///
    /// **Bloqueia** até o usuário soltar o botão: é o laço modal do OLE.
    ///
    /// Duas condições, e as duas são load-bearing:
    ///
    /// 1. Tem de ser chamada da **thread que é dona da janela**.
    /// 2. O botão do mouse tem de estar **pressionado** quando ela começa.
    ///
    /// A segunda foi aprendida na marra: sem botão pressionado o
    /// `DoDragDrop` **não** volta na hora — ele captura o mouse e fica
    /// esperando input que nunca vem, porque o `QueryContinueDrag` só é
    /// chamado quando chega uma mensagem. Um teste que a chamasse sem gesto
    /// prenderia o mouse até o processo morrer.
    ///
    /// Devolve `true` se o arquivo foi solto em algum lugar.
    pub fn drag_file(path: &Path) -> bool {
        let mut dados = DataObject {
            vtbl: &DATA_VTBL,
            refs: 1,
            dropfiles: dropfiles_blob(path),
        };
        let mut fonte = DropSource { vtbl: &DROP_VTBL, refs: 1 };

        // SAFETY: os dois objetos vivem nesta pilha durante a chamada inteira,
        // que é síncrona. Toda inicialização bem-sucedida é desfeita ao sair,
        // inclusive o `S_FALSE` de quando a thread já estava inicializada pelo
        // winit: é o que o contrato do OLE pede, e assim a contagem volta ao
        // nível dele em vez de subir um a cada arrasto.
        unsafe {
            let init = OleInitialize(std::ptr::null());
            let mut efeito = 0u32;
            let hr = DoDragDrop(
                &mut dados as *mut _ as *mut c_void,
                &mut fonte as *mut _ as *mut c_void,
                DROPEFFECT_COPY,
                &mut efeito,
            );
            if init == S_OK || init == S_FALSE {
                OleUninitialize();
            }
            hr == DRAGDROP_S_DROP && efeito != 0
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// O cabeçalho tem 20 bytes fixos, e a lista acaba em dois nulos —
        /// um só faria o destino ler além do caminho procurando o próximo.
        #[test]
        fn o_blob_de_dropfiles_termina_em_dois_nulos() {
            let blob = dropfiles_blob(Path::new(r"C:\tmp\a.png"));

            assert_eq!(u32::from_le_bytes(blob[0..4].try_into().unwrap()), 20);
            assert_eq!(i32::from_le_bytes(blob[16..20].try_into().unwrap()), 1, "fWide");

            let caminho: Vec<u16> = blob[20..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|par| u16::from_le_bytes(*par))
                .collect();
            assert_eq!(&caminho[caminho.len() - 2..], &[0, 0]);
            assert_eq!(
                String::from_utf16_lossy(&caminho[..caminho.len() - 2]),
                r"C:\tmp\a.png"
            );
        }
    }
}
