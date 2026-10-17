//! Arrow C Data Interface: <https://arrow.apache.org/docs/format/CDataInterface.html>
//!
//! Estas structs son el contrato con C, asi que su forma NO se puede cambiar:
//! mismo orden de campos, mismos tipos, mismo tamano.

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr;

use crate::memory::Buffer;
use crate::{Chunk, Column, DataType, Error, Field, Schema};

/// El campo acepta nulos (`ARROW_FLAG_NULLABLE` en la especificacion).
pub const FLAG_NULLABLE: i64 = 2;

/// Descripcion de una columna: nombre, tipo y si admite nulos.
///
/// `#[repr(C)]` obliga a Rust a colocar los campos en memoria en el orden en que
/// estan escritos, como lo haria C. Sin eso Rust puede reordenarlos y el otro
/// lado leeria cada campo en el sitio equivocado.
#[repr(C)]
pub struct ArrowSchema {
    /// Tipo, como texto de C terminado en cero: "i", "l", "g", "u", "b", "+s".
    pub format: *const c_char,
    /// Nombre de la columna (texto C), o NULL.
    pub name: *const c_char,
    /// Metadatos opcionales. No los usamos: siempre NULL.
    pub metadata: *const c_char,
    /// Banderas; por ahora solo `FLAG_NULLABLE`.
    pub flags: i64,
    /// Cuantos hijos tiene (un `Chunk` exportado es un struct con N hijos).
    pub n_children: i64,
    /// Arreglo de punteros a los hijos.
    pub children: *mut *mut ArrowSchema,
    /// Diccionario. Fuera de alcance: siempre NULL.
    pub dictionary: *mut ArrowSchema,
    /// Quien recibe esta struct llama esto UNA vez al terminar. Despues de
    /// llamarlo debe quedar en `None` (NULL en C). `Option<fn>` ocupa lo mismo
    /// que un puntero a funcion, y `None` es exactamente NULL.
    pub release: Option<unsafe extern "C" fn(*mut ArrowSchema)>,
    /// Espacio libre del productor: aqui guardaremos lo que `release` debe soltar.
    pub private_data: *mut c_void,
}

/// Los datos de una columna: longitud, nulos y punteros a los bloques de memoria.
#[repr(C)]
pub struct ArrowArray {
    /// Cantidad de valores logicos.
    pub length: i64,
    /// Cantidad de nulos, o -1 si "no se ha calculado" (el que importa debe contar).
    pub null_count: i64,
    /// Cuantos valores se saltan desde el inicio fisico de los buffers.
    pub offset: i64,
    /// Cantidad de punteros en `buffers`.
    pub n_buffers: i64,
    /// Cantidad de hijos.
    pub n_children: i64,
    /// Punteros a los bloques: [validez, valores] o [validez, offsets, datos].
    /// El de validez es NULL si no hay nulos.
    pub buffers: *mut *const c_void,
    /// Arreglo de punteros a las columnas hijas.
    pub children: *mut *mut ArrowArray,
    /// Diccionario. Fuera de alcance: siempre NULL.
    pub dictionary: *mut ArrowArray,
    /// Igual que en `ArrowSchema`: se llama una vez, y despues queda en `None`.
    pub release: Option<unsafe extern "C" fn(*mut ArrowArray)>,
    /// Lo que `release` necesita para soltar los buffers.
    pub private_data: *mut c_void,
}

/// Texto de formato Arrow para un tipo. Es un literal `c"..."`: ya viene con el
/// cero final que C espera, y vive para siempre.
#[must_use]
pub fn format_of(dtype: DataType) -> &'static CStr {
    match dtype {
        DataType::Bool => c"b",
        DataType::Int32 => c"i",
        DataType::Int64 => c"l",
        DataType::Float64 => c"g",
        DataType::Utf8 => c"u",
    }
}

/// Camino inverso: de texto de formato a tipo.
///
/// # Errors
/// [`Error::Unsupported`] si el formato existe en Arrow pero geoda no lo maneja
/// todavia (fechas, listas, decimales...). No es un dato corrupto, es algo que
/// aun no soportamos.
pub fn datatype_of(format: &CStr) -> Result<DataType, Error> {
    match format.to_bytes() {
        b"b" => Ok(DataType::Bool),
        b"i" => Ok(DataType::Int32),
        b"l" => Ok(DataType::Int64),
        b"g" => Ok(DataType::Float64),
        b"u" => Ok(DataType::Utf8),
        otro => Err(Error::Unsupported(format!(
            "formato Arrow no soportado: {}",
            String::from_utf8_lossy(otro)
        ))),
    }
}

/// Lo que el productor guarda en `private_data` para poder soltarlo en `release`.
struct SchemaPrivate {
    /// Dueno del texto al que apunta `ArrowSchema.name`.
    name: CString,
    /// Hijos, cada uno en el heap con `Box::into_raw`. Dueno de la memoria de
    /// `ArrowSchema.children[i]`.
    children: Vec<*mut ArrowSchema>,
}

/// Arma un `ArrowSchema` con su `private_data` y su `release`.
fn build_schema(
    format: &'static CStr,
    name: &str,
    flags: i64,
    children: Vec<ArrowSchema>,
) -> Result<ArrowSchema, Error> {
    // Si algo falla aqui, los hijos ya exportados no tienen dueno: hay que
    // liberarlos con su `release` antes de devolver el error, o se fugan.
    let (name, n_children) = match (CString::new(name), i64::try_from(children.len())) {
        (Ok(name), Ok(n)) => (name, n),
        (Err(_), _) => {
            release_schemas(children);
            return Err(Error::InvalidArgument(format!(
                "el nombre {name:?} contiene un byte 0"
            )));
        }
        (_, Err(_)) => {
            release_schemas(children);
            return Err(Error::InvalidArgument("demasiados hijos".into()));
        }
    };
    let children: Vec<*mut ArrowSchema> = children
        .into_iter()
        .map(|c| Box::into_raw(Box::new(c)))
        .collect();

    let mut private = Box::new(SchemaPrivate { name, children });
    let schema = ArrowSchema {
        format: format.as_ptr(),
        // El texto y el arreglo viven en el heap, asi que no se mueven cuando
        // `private` cambia de lugar: los punteros siguen siendo validos.
        name: private.name.as_ptr(),
        metadata: ptr::null(),
        flags,
        n_children,
        children: if private.children.is_empty() {
            ptr::null_mut()
        } else {
            private.children.as_mut_ptr()
        },
        dictionary: ptr::null_mut(),
        release: Some(release_schema),
        private_data: Box::into_raw(private).cast::<c_void>(),
    };
    Ok(schema)
}

/// Exporta un campo como un `ArrowSchema` hoja (sin hijos).
///
/// Quien lo reciba debe llamar a su `release` exactamente una vez.
///
/// # Errors
/// [`Error::InvalidArgument`] si el nombre contiene un byte 0 (C lo leeria cortado).
pub fn export_field(field: &Field) -> Result<ArrowSchema, Error> {
    let flags = if field.nullable() { FLAG_NULLABLE } else { 0 };
    build_schema(format_of(field.dtype()), field.name(), flags, Vec::new())
}

/// Exporta un esquema como un struct `+s` con un hijo por campo.
///
/// # Errors
/// Los de [`export_field`].
pub fn export_schema(schema: &Schema) -> Result<ArrowSchema, Error> {
    let mut children = Vec::with_capacity(schema.len());
    for field in schema.fields() {
        match export_field(field) {
            Ok(child) => children.push(child),
            Err(e) => {
                release_schemas(children);
                return Err(e);
            }
        }
    }
    build_schema(c"+s", "", 0, children)
}

/// Libera con su `release` unos `ArrowSchema` que nadie va a usar.
fn release_schemas(schemas: Vec<ArrowSchema>) {
    for mut schema in schemas {
        if let Some(release) = schema.release {
            // SAFETY: cada uno salio de `build_schema` y aun no se libero.
            unsafe { release(&raw mut schema) };
        }
    }
}

/// `release` de todo `ArrowSchema` que exportamos nosotros.
///
/// Lo llama el consumidor, una sola vez. Suelta el nombre, los hijos y deja la
/// struct marcada como liberada (`release = None`).
///
/// # Safety
/// `schema` debe apuntar a un `ArrowSchema` creado por `build_schema` que aun
/// no ha sido liberado.
unsafe extern "C" fn release_schema(schema: *mut ArrowSchema) {
    // SAFETY: por el contrato de arriba, `schema` es valido y exclusivo.
    let schema = unsafe { &mut *schema };
    if schema.release.is_none() {
        // Ya estaba liberado: la especificacion no permite llamarlo dos veces,
        // pero ignorarlo es mas seguro que liberar dos veces la misma memoria.
        return;
    }
    // SAFETY: `private_data` salio de `Box::into_raw` en `build_schema` y,
    // como `release` era `Some`, nadie lo ha devuelto todavia.
    let private = unsafe { Box::from_raw(schema.private_data.cast::<SchemaPrivate>()) };
    for &child in &private.children {
        // SAFETY: cada hijo salio de `Box::into_raw` en `build_schema`.
        // El padre es quien libera a sus hijos; el consumidor nunca lo hace.
        unsafe {
            if let Some(release) = (*child).release {
                release(child);
            }
            drop(Box::from_raw(child));
        }
    }
    schema.release = None;
    schema.private_data = ptr::null_mut();
    schema.children = ptr::null_mut();
    schema.n_children = 0;
    // `private` (y con el, `name`) se libera al salir de la funcion.
}

/// Lo que el productor guarda en `private_data` de un `ArrowArray`.
struct ArrayPrivate {
    /// Los punteros que se publican en `ArrowArray.buffers`.
    buffer_ptrs: Vec<*const c_void>,
    /// Clones de los `Buffer` a los que apuntan `buffer_ptrs`. No se leen: estan
    /// para que la memoria siga viva hasta el `release` (cada clon suma uno al `Arc`).
    _owners: Vec<Buffer>,
    /// Hijos, cada uno en el heap con `Box::into_raw`.
    children: Vec<*mut ArrowArray>,
}

/// Arma un `ArrowArray` que comparte (sin copiar) los buffers que recibe.
fn build_array(
    length: usize,
    null_count: usize,
    offset: usize,
    validity: Option<&Buffer>,
    data: &[Buffer],
    children: Vec<ArrowArray>,
) -> Result<ArrowArray, Error> {
    let to_i64 = |n: usize| {
        i64::try_from(n).map_err(|_| Error::InvalidArgument(format!("{n} no cabe en un i64")))
    };
    // Buffer 0 = validez (NULL si no hay), despues los de datos, como pide Arrow.
    let mut buffer_ptrs: Vec<*const c_void> = Vec::with_capacity(1 + data.len());
    buffer_ptrs.push(validity.map_or(ptr::null(), |v| v.as_ptr().cast::<c_void>()));
    buffer_ptrs.extend(data.iter().map(|b| b.as_ptr().cast::<c_void>()));

    let numeros = (|| {
        Ok::<_, Error>((
            to_i64(length)?,
            to_i64(null_count)?,
            to_i64(offset)?,
            to_i64(buffer_ptrs.len())?,
            to_i64(children.len())?,
        ))
    })();
    let (length, null_count, offset, n_buffers, n_children) = match numeros {
        Ok(n) => n,
        Err(e) => {
            release_arrays(children);
            return Err(e);
        }
    };

    let owners: Vec<Buffer> = validity.into_iter().chain(data).cloned().collect();
    let children: Vec<*mut ArrowArray> = children
        .into_iter()
        .map(|c| Box::into_raw(Box::new(c)))
        .collect();
    let mut private = Box::new(ArrayPrivate {
        buffer_ptrs,
        _owners: owners,
        children,
    });
    Ok(ArrowArray {
        length,
        null_count,
        offset,
        n_buffers,
        n_children,
        // Viven en el heap: no se mueven cuando `private` cambia de lugar.
        buffers: private.buffer_ptrs.as_mut_ptr(),
        children: if private.children.is_empty() {
            ptr::null_mut()
        } else {
            private.children.as_mut_ptr()
        },
        dictionary: ptr::null_mut(),
        release: Some(release_array),
        private_data: Box::into_raw(private).cast::<c_void>(),
    })
}

/// Libera con su `release` unos `ArrowArray` que nadie va a usar.
fn release_arrays(arrays: Vec<ArrowArray>) {
    for mut array in arrays {
        if let Some(release) = array.release {
            // SAFETY: cada uno salio de `build_array` y aun no se libero.
            unsafe { release(&raw mut array) };
        }
    }
}

/// Exporta una columna sin copiar: el `ArrowArray` comparte sus buffers.
///
/// Quien lo reciba debe llamar a su `release` exactamente una vez; hasta
/// entonces los datos siguen vivos aunque se suelte la `Column`.
///
/// # Errors
/// [`Error::InvalidArgument`] si algun tamano no cabe en un `i64`.
pub fn export_column(column: &Column) -> Result<ArrowArray, Error> {
    build_array(
        column.len(),
        column.null_count(),
        column.offset(),
        column.validity(),
        column.buffers(),
        Vec::new(),
    )
}

/// Exporta un `Chunk` como un array struct (`+s`) con una columna hija por campo.
///
/// # Errors
/// Los de [`export_column`].
pub fn export_chunk(chunk: &Chunk) -> Result<ArrowArray, Error> {
    let mut children = Vec::with_capacity(chunk.columns().len());
    for column in chunk.columns() {
        match export_column(column) {
            Ok(child) => children.push(child),
            Err(e) => {
                release_arrays(children);
                return Err(e);
            }
        }
    }
    // Un struct solo lleva el buffer de validez (NULL: las filas no son nulas).
    build_array(chunk.len(), 0, 0, None, &[], children)
}

/// `release` de todo `ArrowArray` que exportamos nosotros.
///
/// # Safety
/// `array` debe apuntar a un `ArrowArray` creado por `build_array` que aun no ha
/// sido liberado.
unsafe extern "C" fn release_array(array: *mut ArrowArray) {
    // SAFETY: por el contrato de arriba, `array` es valido y exclusivo.
    let array = unsafe { &mut *array };
    if array.release.is_none() {
        return;
    }
    // SAFETY: `private_data` salio de `Box::into_raw` en `build_array` y, como
    // `release` era `Some`, nadie lo ha devuelto todavia.
    let private = unsafe { Box::from_raw(array.private_data.cast::<ArrayPrivate>()) };
    for &child in &private.children {
        // SAFETY: cada hijo salio de `Box::into_raw` en `build_array`.
        unsafe {
            if let Some(release) = (*child).release {
                release(child);
            }
            drop(Box::from_raw(child));
        }
    }
    array.release = None;
    array.private_data = ptr::null_mut();
    array.buffers = ptr::null_mut();
    array.children = ptr::null_mut();
    array.n_buffers = 0;
    array.n_children = 0;
    // Al salir, `private` suelta los clones de `Buffer`: baja el contador de `Arc`
    // y, si ya nadie mas los usa, se libera la memoria.
}

// ---------------------------------------------------------------------------
// Importar: de lo que manda otro programa a un `Chunk` de geoda.
//
// Aqui NADA de lo que llega es de fiar. Reglas:
// - todo numero se valida antes de usarlo como tamano o indice;
// - los datos se COPIAN a buffers propios (alineados a 64): no se usan los
//   punteros ajenos despues de volver, asi que no hay que alinear nada;
// - si algo falla se devuelve `Err` y NO se toma propiedad: el productor sigue
//   siendo dueno y quien llamo debe seguir llamando a su `release`;
// - si todo sale bien, se llama a `release` del array una vez (ya copiamos).
// ---------------------------------------------------------------------------

/// Convierte en `Ffi` los errores de datos que llegaron por la frontera. `Unsupported`
/// y `Alloc` se quedan como estan: no son datos corruptos.
fn ffi(e: Error) -> Error {
    match e {
        Error::Unsupported(_) | Error::Alloc(_) => e,
        otro => Error::Ffi(otro.to_string()),
    }
}

/// Entero de la frontera a `usize`, rechazando negativos.
fn non_negative(n: i64, que: &str) -> Result<usize, Error> {
    usize::try_from(n).map_err(|_| Error::Ffi(format!("{que} invalido: {n}")))
}

/// Texto C a `&CStr`, rechazando NULL.
///
/// # Safety
/// Si `p` no es NULL debe apuntar a un texto terminado en cero valido mientras
/// dure `'a`.
unsafe fn c_str<'a>(p: *const c_char, que: &str) -> Result<&'a CStr, Error> {
    if p.is_null() {
        return Err(Error::Ffi(format!("{que} es NULL")));
    }
    // SAFETY: no es NULL y el contrato de arriba dice que termina en cero.
    Ok(unsafe { CStr::from_ptr(p) })
}

/// Copia `nbytes` desde `p` a un `Buffer` propio. Con 0 bytes no toca `p`.
///
/// # Safety
/// Si `nbytes > 0`, `p` debe ser valido para leer `nbytes` bytes.
unsafe fn copy_bytes(p: *const c_void, nbytes: usize, que: &str) -> Result<Buffer, Error> {
    if nbytes == 0 {
        return Buffer::from_bytes(&[]);
    }
    if p.is_null() {
        return Err(Error::Ffi(format!(
            "{que}: puntero NULL pero se esperaban {nbytes} bytes"
        )));
    }
    // SAFETY: no es NULL y el llamador garantiza `nbytes` bytes legibles. Se lee
    // como `u8`, que no exige alineacion: sirve aunque el productor no alinee.
    let bytes = unsafe { std::slice::from_raw_parts(p.cast::<u8>(), nbytes) };
    Buffer::from_bytes(bytes)
}

/// Campo de geoda a partir de un `ArrowSchema` hoja.
///
/// # Safety
/// Los punteros de `schema` deben ser validos segun la especificacion.
unsafe fn field_of(schema: &ArrowSchema) -> Result<Field, Error> {
    // SAFETY: contrato de la funcion.
    let dtype = datatype_of(unsafe { c_str(schema.format, "format") }?)?;
    if schema.n_children != 0 || !schema.dictionary.is_null() {
        return Err(Error::Unsupported(
            "tipos anidados o con diccionario".into(),
        ));
    }
    // SAFETY: contrato de la funcion.
    let name = unsafe { c_str(schema.name, "name") }?
        .to_str()
        .map_err(|_| Error::Ffi("name no es UTF-8".into()))?;
    Ok(Field::new(name, dtype, schema.flags & FLAG_NULLABLE != 0))
}

/// Importa un `ArrowSchema` struct (`+s`) como `Schema`. No lo libera: sigue siendo
/// del llamador.
///
/// # Errors
/// - [`Error::Unsupported`] si no es un struct o algun hijo es de un tipo que
///   geoda aun no maneja.
/// - [`Error::Ffi`] si los datos estan mal formados (NULL, conteos negativos,
///   nombres repetidos...).
///
/// # Safety
/// Todos los punteros de `schema` y de sus hijos deben ser validos segun la
/// especificacion de la Arrow C Data Interface.
pub unsafe fn import_schema(schema: &ArrowSchema) -> Result<Schema, Error> {
    // SAFETY: contrato de la funcion.
    if unsafe { c_str(schema.format, "format") }? != c"+s" {
        return Err(Error::Unsupported("se esperaba un struct (+s)".into()));
    }
    if !schema.dictionary.is_null() {
        return Err(Error::Unsupported("diccionarios".into()));
    }
    let n = non_negative(schema.n_children, "n_children")?;
    if n > 0 && schema.children.is_null() {
        return Err(Error::Ffi("children es NULL con hijos declarados".into()));
    }
    let mut fields = Vec::with_capacity(n);
    for i in 0..n {
        // SAFETY: `children` tiene `n` punteros (contrato de la funcion).
        let child = unsafe { *schema.children.add(i) };
        if child.is_null() {
            return Err(Error::Ffi(format!("el hijo {i} es NULL")));
        }
        // SAFETY: no es NULL y es valido (contrato de la funcion).
        fields.push(unsafe { field_of(&*child) }?);
    }
    Schema::try_new(fields).map_err(ffi)
}

/// Columna a partir de un par `ArrowSchema` / `ArrowArray` hoja, copiando los datos.
/// No libera nada.
///
/// # Safety
/// Los punteros de `schema` y `array` deben ser validos segun la especificacion,
/// y cada buffer de `array` debe poder leerse completo para `offset + length`.
unsafe fn column_of(schema: &ArrowSchema, array: &ArrowArray) -> Result<Column, Error> {
    // SAFETY: contrato de la funcion.
    let dtype = datatype_of(unsafe { c_str(schema.format, "format") }?)?;
    if schema.n_children != 0 || array.n_children != 0 {
        return Err(Error::Unsupported("tipos anidados".into()));
    }
    if !array.dictionary.is_null() || !schema.dictionary.is_null() {
        return Err(Error::Unsupported("diccionarios".into()));
    }
    if array.release.is_none() {
        return Err(Error::Ffi("el array ya estaba liberado".into()));
    }
    let length = non_negative(array.length, "length")?;
    let offset = non_negative(array.offset, "offset")?;
    if array.null_count < -1 {
        return Err(Error::Ffi(format!(
            "null_count invalido: {}",
            array.null_count
        )));
    }
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::Ffi("offset + length desborda".into()))?;

    let esperados: i64 = if dtype == DataType::Utf8 { 3 } else { 2 };
    if array.n_buffers != esperados || array.buffers.is_null() {
        return Err(Error::Ffi(format!(
            "{dtype:?} lleva {esperados} buffers y llegaron {}",
            array.n_buffers
        )));
    }
    // SAFETY: `buffers` no es NULL y tiene `n_buffers` punteros.
    let ptr_de = |i: usize| unsafe { *array.buffers.add(i) };

    let bits = end.div_ceil(8);
    // Sin puntero de validez = sin nulos.
    // SAFETY: lee `bits` bytes (contrato de la funcion).
    let validity = if ptr_de(0).is_null() {
        None
    } else {
        Some(unsafe { copy_bytes(ptr_de(0), bits, "validez") }?)
    };

    let ancho = |w: usize| {
        end.checked_mul(w)
            .ok_or_else(|| Error::Ffi("valores * ancho desborda".into()))
    };
    // SAFETY (todos los `copy_bytes` de abajo): el contrato de la funcion dice que
    // cada buffer se puede leer completo para `offset + length` valores.
    let buffers = match dtype {
        DataType::Bool => vec![unsafe { copy_bytes(ptr_de(1), bits, "valores") }?],
        DataType::Int32 => vec![unsafe { copy_bytes(ptr_de(1), ancho(4)?, "valores") }?],
        DataType::Int64 | DataType::Float64 => {
            vec![unsafe { copy_bytes(ptr_de(1), ancho(8)?, "valores") }?]
        }
        DataType::Utf8 => {
            // Un array vacio puede venir sin offsets: se sintetiza el unico [0].
            let offsets = if end == 0 {
                Buffer::from_bytes(&0_i32.to_ne_bytes())?
            } else {
                unsafe { copy_bytes(ptr_de(1), (end + 1) * 4, "offsets") }?
            };
            // Ya es nuestra copia (alineada), asi que se puede leer como [i32].
            let ultimo = *offsets
                .typed::<i32>()?
                .get(end)
                .ok_or_else(|| Error::Ffi("faltan offsets".into()))?;
            let ultimo = non_negative(i64::from(ultimo), "ultimo offset")?;
            vec![offsets, unsafe { copy_bytes(ptr_de(2), ultimo, "datos") }?]
        }
    };

    let columna = Column::try_new(dtype, length, offset, validity, buffers).map_err(ffi)?;
    // Con -1 el productor dice "cuentalo tu": ya lo calculo `Column`. Con un valor
    // concreto, debe coincidir con los bits.
    if let Ok(declarado) = usize::try_from(array.null_count)
        && declarado != columna.null_count()
    {
        return Err(Error::Ffi(format!(
            "null_count declarado {declarado}, pero los bits dan {}",
            columna.null_count()
        )));
    }
    Ok(columna)
}

/// Importa un `Chunk` desde un `ArrowSchema` y un `ArrowArray` struct (`+s`).
///
/// Copia los datos; si todo sale bien llama al `release` de `array` (una vez) y
/// queda en `None`. El `schema` solo se lee: sigue siendo del llamador, que debe
/// liberarlo aparte.
///
/// Si falla, devuelve `Err` y **no toma propiedad**: `array.release` sigue
/// intacto y el llamador debe llamarlo.
///
/// # Errors
/// - [`Error::Unsupported`] para tipos que geoda aun no maneja.
/// - [`Error::Ffi`] para datos mal formados: conteos negativos o inconsistentes,
///   punteros NULL donde hacen falta, `null_count` que no coincide con los bits,
///   offsets Utf8 invalidos, longitudes de hijos distintas...
///
/// # Safety
/// Los punteros de `schema` y `array` (y los de sus hijos y buffers) deben ser
/// validos segun la especificacion de la Arrow C Data Interface. El formato no
/// lleva el tamano de los buffers, asi que se confia en que `offset + length`
/// describa memoria legible: lo que si se valida es que sea coherente consigo misma.
pub unsafe fn import_chunk(schema: &ArrowSchema, array: &mut ArrowArray) -> Result<Chunk, Error> {
    // SAFETY: contrato de la funcion.
    let esquema = unsafe { import_schema(schema) }?;
    if array.release.is_none() {
        return Err(Error::Ffi("el array ya estaba liberado".into()));
    }
    let n = non_negative(array.n_children, "n_children")?;
    if n != esquema.len() {
        return Err(Error::Ffi(format!(
            "el esquema tiene {} campos y el array {n} hijos",
            esquema.len()
        )));
    }
    if array.offset != 0 {
        return Err(Error::Unsupported("struct con offset distinto de 0".into()));
    }
    if array.null_count > 0 {
        return Err(Error::Unsupported("struct con filas nulas".into()));
    }
    if array.n_buffers != 1 || array.buffers.is_null() {
        return Err(Error::Ffi(format!(
            "un struct lleva 1 buffer y llegaron {}",
            array.n_buffers
        )));
    }
    // SAFETY: `buffers` no es NULL y tiene 1 puntero.
    if !unsafe { *array.buffers }.is_null() {
        return Err(Error::Unsupported("struct con validez propia".into()));
    }
    let length = non_negative(array.length, "length")?;
    if n > 0 && array.children.is_null() {
        return Err(Error::Ffi("children es NULL con hijos declarados".into()));
    }

    let mut columnas = Vec::with_capacity(n);
    for i in 0..n {
        // SAFETY: ambos arreglos tienen `n` punteros (contrato de la funcion).
        let (hijo_s, hijo_a) = unsafe { (*schema.children.add(i), *array.children.add(i)) };
        if hijo_a.is_null() {
            return Err(Error::Ffi(format!("el hijo {i} del array es NULL")));
        }
        // SAFETY: no son NULL (hijo_s lo valido `import_schema`) y son validos.
        let columna = unsafe { column_of(&*hijo_s, &*hijo_a) }?;
        if columna.len() != length {
            return Err(Error::Ffi(format!(
                "el hijo {i} tiene {} valores y el struct {length}",
                columna.len()
            )));
        }
        columnas.push(columna);
    }
    let chunk = Chunk::try_new(esquema, columnas).map_err(ffi)?;

    // Todo copiado y validado: ahora si, el productor puede soltar lo suyo.
    if let Some(release) = array.release {
        // SAFETY: `array` es valido y su `release` aun no se ha llamado.
        unsafe { release(array) };
    }
    Ok(chunk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoolBuilder, PrimitiveBuilder, Utf8Builder};

    #[test]
    fn formato_ida_y_vuelta_para_los_cinco_tipos() {
        for tipo in [
            DataType::Bool,
            DataType::Int32,
            DataType::Int64,
            DataType::Float64,
            DataType::Utf8,
        ] {
            assert_eq!(datatype_of(format_of(tipo)), Ok(tipo));
        }
    }

    #[test]
    fn formato_desconocido_es_unsupported() {
        assert!(matches!(datatype_of(c"tdD"), Err(Error::Unsupported(_))));
    }

    /// Texto C a `&str`, solo para las pruebas.
    fn texto<'a>(p: *const c_char) -> &'a str {
        // SAFETY: las pruebas solo pasan punteros de un ArrowSchema vivo.
        unsafe { CStr::from_ptr(p) }.to_str().unwrap()
    }

    #[test]
    fn exportar_un_campo() {
        let mut s = export_field(&Field::new("edad", DataType::Int32, true)).unwrap();
        assert_eq!(texto(s.format), "i");
        assert_eq!(texto(s.name), "edad");
        assert_eq!(s.flags, FLAG_NULLABLE);
        assert_eq!(s.n_children, 0);
        assert!(s.children.is_null() && s.dictionary.is_null());

        // El consumidor libera una vez; la struct queda marcada como liberada.
        let release = s.release.expect("recien exportado tiene release");
        // SAFETY: `s` es valido y es la primera vez que se libera.
        unsafe { release(&raw mut s) };
        assert!(s.release.is_none());
    }

    #[test]
    fn campo_no_nullable_no_lleva_la_bandera() {
        let mut s = export_field(&Field::new("id", DataType::Int64, false)).unwrap();
        assert_eq!(s.flags, 0);
        // SAFETY: primera y unica liberacion.
        unsafe { s.release.unwrap()(&raw mut s) };
    }

    #[test]
    fn exportar_un_esquema_con_hijos() {
        let schema = Schema::try_new(vec![
            Field::new("a", DataType::Bool, false),
            Field::new("b", DataType::Utf8, true),
        ])
        .unwrap();
        let mut s = export_schema(&schema).unwrap();
        assert_eq!(texto(s.format), "+s");
        assert_eq!(s.n_children, 2);

        // SAFETY: `children` tiene `n_children` punteros validos mientras no se libere.
        let (a, b) = unsafe { (&**s.children, &**s.children.add(1)) };
        assert_eq!((texto(a.format), texto(a.name), a.flags), ("b", "a", 0));
        assert_eq!(
            (texto(b.format), texto(b.name), b.flags),
            ("u", "b", FLAG_NULLABLE)
        );

        // SAFETY: primera y unica liberacion; libera tambien a los hijos.
        unsafe { s.release.unwrap()(&raw mut s) };
        assert!(s.release.is_none() && s.children.is_null() && s.n_children == 0);
    }

    #[test]
    fn nombre_con_byte_cero_es_err() {
        let r = export_field(&Field::new("a\0b", DataType::Bool, true));
        assert!(matches!(r, Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn exportar_esquema_con_un_hijo_invalido_es_err_sin_fugar_los_otros() {
        let schema = Schema::try_new(vec![
            Field::new("bien", DataType::Int32, true),
            Field::new("mal\0nombre", DataType::Int32, true),
        ])
        .unwrap();
        // El primer hijo ya estaba exportado cuando falla el segundo: debe liberarse.
        // (La fuga solo la detecta Miri; aqui se comprueba el `Err`.)
        assert!(matches!(
            export_schema(&schema),
            Err(Error::InvalidArgument(_))
        ));
    }

    /// Puntero al buffer `i` de un `ArrowArray`.
    fn buffer_de(a: &ArrowArray, i: usize) -> *const c_void {
        // SAFETY: las pruebas solo piden `i < n_buffers` de un array vivo.
        unsafe { *a.buffers.add(i) }
    }

    /// `n` valores `T` leidos del buffer `i`.
    fn leer<T: Copy>(a: &ArrowArray, i: usize, n: usize) -> Vec<T> {
        // SAFETY: las pruebas piden `n` valores que caben en el buffer.
        unsafe { std::slice::from_raw_parts(buffer_de(a, i).cast::<T>(), n) }.to_vec()
    }

    /// Libera un array exportado, como lo haria el consumidor.
    fn liberar(mut a: ArrowArray) {
        // SAFETY: primera y unica liberacion.
        unsafe { a.release.unwrap()(&raw mut a) };
        assert!(a.release.is_none());
    }

    fn enteros_con_nulos() -> Column {
        let mut b = PrimitiveBuilder::<i32>::new();
        b.append(1);
        b.append_null();
        b.append(3);
        b.finish().unwrap()
    }

    #[test]
    fn exportar_columna_con_nulos() {
        let a = export_column(&enteros_con_nulos()).unwrap();
        assert_eq!((a.length, a.null_count, a.offset), (3, 1, 0));
        assert_eq!((a.n_buffers, a.n_children), (2, 0));
        assert!(a.children.is_null() && a.dictionary.is_null());
        assert_eq!(leer::<u8>(&a, 0, 1), [0b0000_0101]);
        assert_eq!(leer::<i32>(&a, 1, 3), [1, 0, 3]);
        liberar(a);
    }

    #[test]
    fn columna_sin_nulos_publica_validez_null() {
        let mut b = PrimitiveBuilder::<i64>::new();
        b.append(7);
        let a = export_column(&b.finish().unwrap()).unwrap();
        assert_eq!(a.null_count, 0);
        assert!(buffer_de(&a, 0).is_null());
        assert_eq!(leer::<i64>(&a, 1, 1), [7]);
        liberar(a);
    }

    #[test]
    fn exportar_no_copia_y_conserva_el_offset() {
        let columna = enteros_con_nulos();
        let corte = columna.slice(1, 2).unwrap();
        let a = export_column(&corte).unwrap();
        assert_eq!((a.length, a.null_count, a.offset), (2, 1, 1));
        // Mismo bloque de memoria que la columna original: cero copia.
        assert_eq!(
            buffer_de(&a, 1),
            columna.buffers()[0].as_ptr().cast::<c_void>()
        );
        liberar(a);
    }

    #[test]
    fn exportar_utf8_lleva_tres_buffers() {
        let mut b = Utf8Builder::new();
        b.append("hola").unwrap();
        b.append("").unwrap();
        b.append("ok").unwrap();
        let a = export_column(&b.finish().unwrap()).unwrap();
        assert_eq!(a.n_buffers, 3);
        assert_eq!(leer::<i32>(&a, 1, 4), [0, 4, 4, 6]);
        assert_eq!(leer::<u8>(&a, 2, 6), *b"holaok");
        liberar(a);
    }

    #[test]
    fn bool_lleva_dos_buffers() {
        let mut b = BoolBuilder::new();
        b.append(true);
        b.append(false);
        b.append(true);
        let a = export_column(&b.finish().unwrap()).unwrap();
        assert_eq!(a.n_buffers, 2);
        assert_eq!(leer::<u8>(&a, 1, 1), [0b0000_0101]);
        liberar(a);
    }

    #[test]
    fn los_datos_sobreviven_a_la_columna_hasta_el_release() {
        let columna = enteros_con_nulos();
        let a = export_column(&columna).unwrap();
        drop(columna);
        // Si el export no hubiera clonado los buffers, esto leeria memoria liberada
        // (Miri lo detecta; aqui comprueba que los valores siguen ahi).
        assert_eq!(leer::<i32>(&a, 1, 3), [1, 0, 3]);
        liberar(a);
    }

    #[test]
    fn exportar_un_chunk_como_struct() {
        let mut texto = Utf8Builder::new();
        texto.append("a").unwrap();
        texto.append("b").unwrap();
        texto.append("c").unwrap();
        let schema = Schema::try_new(vec![
            Field::new("n", DataType::Int32, true),
            Field::new("t", DataType::Utf8, false),
        ])
        .unwrap();
        let chunk =
            Chunk::try_new(schema, vec![enteros_con_nulos(), texto.finish().unwrap()]).unwrap();

        let a = export_chunk(&chunk).unwrap();
        assert_eq!((a.length, a.null_count, a.offset), (3, 0, 0));
        assert_eq!((a.n_buffers, a.n_children), (1, 2));
        assert!(buffer_de(&a, 0).is_null());
        // SAFETY: `children` tiene 2 punteros validos mientras no se libere.
        let (n, t) = unsafe { (&**a.children, &**a.children.add(1)) };
        assert_eq!((n.length, n.null_count, n.n_buffers), (3, 1, 2));
        assert_eq!((t.length, t.null_count, t.n_buffers), (3, 0, 3));
        liberar(a);
    }

    // ---- Importar -------------------------------------------------------

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// `release` de prueba: cuenta las llamadas y marca el array como liberado.
    unsafe extern "C" fn release_contado(a: *mut ArrowArray) {
        // SAFETY: el test pasa un array valido.
        let a = unsafe { &mut *a };
        // SAFETY: `private_data` apunta a un `AtomicUsize` que vive mas que el array.
        unsafe { &*a.private_data.cast::<AtomicUsize>() }.fetch_add(1, Ordering::SeqCst);
        a.release = None;
    }

    /// Array hoja armado a mano, como lo mandaria otro programa. `ptrs` debe
    /// vivir mientras se use el array.
    fn hoja(
        length: i64,
        null_count: i64,
        offset: i64,
        ptrs: &mut [*const c_void],
        cuenta: &AtomicUsize,
    ) -> ArrowArray {
        ArrowArray {
            length,
            null_count,
            offset,
            n_buffers: i64::try_from(ptrs.len()).unwrap(),
            n_children: 0,
            buffers: ptrs.as_mut_ptr(),
            children: ptr::null_mut(),
            dictionary: ptr::null_mut(),
            release: Some(release_contado),
            private_data: ptr::from_ref(cuenta).cast_mut().cast::<c_void>(),
        }
    }

    /// Array struct armado a mano.
    fn estructura(
        length: i64,
        hijos: &mut [*mut ArrowArray],
        buffers: &mut [*const c_void; 1],
        cuenta: &AtomicUsize,
    ) -> ArrowArray {
        ArrowArray {
            length,
            null_count: 0,
            offset: 0,
            n_buffers: 1,
            n_children: i64::try_from(hijos.len()).unwrap(),
            buffers: buffers.as_mut_ptr(),
            children: hijos.as_mut_ptr(),
            dictionary: ptr::null_mut(),
            release: Some(release_contado),
            private_data: ptr::from_ref(cuenta).cast_mut().cast::<c_void>(),
        }
    }

    fn esquema_hoja(dtype: DataType) -> ArrowSchema {
        export_field(&Field::new("x", dtype, true)).unwrap()
    }

    fn liberar_schema(mut s: ArrowSchema) {
        // SAFETY: primera y unica liberacion.
        unsafe { s.release.unwrap()(&raw mut s) };
    }

    #[test]
    fn roundtrip_de_un_chunk_con_nulos_y_offset() {
        let mut texto = Utf8Builder::new();
        texto.append("x").unwrap();
        texto.append_null();
        texto.append("z").unwrap();
        let mut booleanos = BoolBuilder::new();
        booleanos.append(true);
        booleanos.append(false);
        booleanos.append(true);
        let enteros = enteros_con_nulos(); // [1, null, 3]
        let schema = Schema::try_new(vec![
            Field::new("n", DataType::Int32, true),
            Field::new("t", DataType::Utf8, true),
            Field::new("b", DataType::Bool, false),
        ])
        .unwrap();
        // Todas las columnas cortadas desde la posicion 1: offset = 1.
        let chunk = Chunk::try_new(
            schema.clone(),
            vec![
                enteros.slice(1, 2).unwrap(),
                texto.finish().unwrap().slice(1, 2).unwrap(),
                booleanos.finish().unwrap().slice(1, 2).unwrap(),
            ],
        )
        .unwrap();

        let esquema_c = export_schema(&schema).unwrap();
        let mut array_c = export_chunk(&chunk).unwrap();
        // SAFETY: ambos los acabamos de exportar nosotros: son validos.
        let importado = unsafe { import_chunk(&esquema_c, &mut array_c) }.unwrap();
        assert!(array_c.release.is_none(), "al importar se libera el array");
        liberar_schema(esquema_c);

        assert_eq!(importado.schema(), &schema);
        assert_eq!(importado.len(), 2);
        let enteros_i = importado.column(0).unwrap();
        assert_eq!((enteros_i.offset(), enteros_i.null_count()), (1, 1));
        assert!(enteros_i.is_null(0).unwrap());
        assert_eq!(enteros_i.values::<i32>().unwrap(), [0, 3]);
        let texto_i = importado.column(1).unwrap();
        assert!(texto_i.is_null(0).unwrap());
        assert_eq!(texto_i.str_value(1).unwrap(), "z");
        let bool_i = importado.column(2).unwrap();
        assert!(!bool_i.bool_value(0).unwrap() && bool_i.bool_value(1).unwrap());
        // Es una copia, y la copia nace alineada a 64.
        let copia = enteros_i.buffers()[0].as_ptr();
        assert_ne!(copia, enteros.buffers()[0].as_ptr());
        assert_eq!(copia as usize % 64, 0);
    }

    #[test]
    fn importar_llama_release_exactamente_una_vez() {
        let s = export_schema(&Schema::try_new(vec![]).unwrap()).unwrap();
        let cuenta = AtomicUsize::new(0);
        let mut b = [ptr::null()];
        let mut a = estructura(0, &mut [], &mut b, &cuenta);
        // SAFETY: structs armadas a mano, validas.
        let chunk = unsafe { import_chunk(&s, &mut a) }.unwrap();
        assert!(chunk.is_empty());
        assert_eq!(cuenta.load(Ordering::SeqCst), 1);
        assert!(a.release.is_none());
        liberar_schema(s);
    }

    #[test]
    fn hijo_de_otra_longitud_es_err_y_no_toma_propiedad() {
        let schema = Schema::try_new(vec![Field::new("a", DataType::Int32, true)]).unwrap();
        let s = export_schema(&schema).unwrap();
        let datos = [1_i32, 2, 3];
        let mut ptrs: [*const c_void; 2] = [ptr::null(), datos.as_ptr().cast()];
        let cuenta_hijo = AtomicUsize::new(0);
        let mut hijo = hoja(3, 0, 0, &mut ptrs, &cuenta_hijo);
        let mut hijos = [&raw mut hijo];
        let cuenta = AtomicUsize::new(0);
        let mut b = [ptr::null()];
        let mut a = estructura(5, &mut hijos, &mut b, &cuenta); // dice 5, el hijo trae 3

        // SAFETY: structs armadas a mano, validas.
        let r = unsafe { import_chunk(&s, &mut a) };
        assert!(matches!(r, Err(Error::Ffi(_))));
        assert_eq!(cuenta.load(Ordering::SeqCst), 0, "no se libero");
        assert!(a.release.is_some(), "el llamador sigue siendo el dueno");

        // Con la longitud correcta el mismo array si se importa.
        a.length = 3;
        // SAFETY: igual que arriba.
        let chunk = unsafe { import_chunk(&s, &mut a) }.unwrap();
        assert_eq!(chunk.column(0).unwrap().values::<i32>().unwrap(), [1, 2, 3]);
        assert_eq!(cuenta.load(Ordering::SeqCst), 1);
        liberar_schema(s);
    }

    #[test]
    fn buffers_sin_alinear_se_copian() {
        /// Bloque alineado a 8: empezar en su byte 1 deja la direccion desalineada
        /// para un `i32` con seguridad, no por casualidad.
        #[repr(align(8))]
        struct Bloque([u8; 16]);

        let mut bloque = Bloque([0; 16]);
        for (i, v) in [10_i32, 20, 30].iter().enumerate() {
            bloque.0[1 + i * 4..5 + i * 4].copy_from_slice(&v.to_ne_bytes());
        }
        // SAFETY: el indice 1 esta dentro del bloque.
        let origen = unsafe { bloque.0.as_ptr().add(1) };
        assert_ne!(origen as usize % 4, 0, "debe estar desalineado para i32");
        let mut ptrs: [*const c_void; 2] = [ptr::null(), origen.cast()];
        let cuenta = AtomicUsize::new(0);
        let a = hoja(3, 0, 0, &mut ptrs, &cuenta);
        let s = esquema_hoja(DataType::Int32);

        // SAFETY: los buffers tienen los 12 bytes que cubre `length`.
        let c = unsafe { column_of(&s, &a) }.unwrap();
        assert_eq!(c.values::<i32>().unwrap(), [10, 20, 30]);
        let copia = c.buffers()[0].as_ptr();
        assert_ne!(copia, origen);
        assert_eq!(copia as usize % 64, 0);
        liberar_schema(s);
    }

    /// Importa una columna Int32 de 3 valores con la validez y el `null_count` dados.
    fn importar_con_validez(validez: Option<u8>, null_count: i64) -> Result<Column, Error> {
        let datos = [1_i32, 2, 3];
        let validez = validez.map(|v| [v]);
        let mut ptrs: [*const c_void; 2] = [
            validez.as_ref().map_or(ptr::null(), |v| v.as_ptr().cast()),
            datos.as_ptr().cast(),
        ];
        let cuenta = AtomicUsize::new(0);
        let a = hoja(3, null_count, 0, &mut ptrs, &cuenta);
        let s = esquema_hoja(DataType::Int32);
        // SAFETY: los buffers cubren `length` valores.
        let r = unsafe { column_of(&s, &a) };
        liberar_schema(s);
        r
    }

    #[test]
    fn null_count_menos_uno_se_calcula() {
        let c = importar_con_validez(Some(0b101), -1).unwrap();
        assert_eq!(c.null_count(), 1);
        assert!(c.is_null(1).unwrap());
    }

    #[test]
    fn null_count_declarado_que_no_coincide_es_err() {
        assert!(matches!(
            importar_con_validez(Some(0b101), 2),
            Err(Error::Ffi(_))
        ));
        // Dice que hay nulos pero no manda bits de validez.
        assert!(matches!(importar_con_validez(None, 1), Err(Error::Ffi(_))));
    }

    #[test]
    fn validez_null_significa_sin_nulos() {
        let c = importar_con_validez(None, 0).unwrap();
        assert_eq!(c.null_count(), 0);
        assert!(c.validity().is_none());
    }

    #[test]
    fn datos_inconsistentes_son_ffi() {
        let datos = [1_i32, 2, 3];
        let s = esquema_hoja(DataType::Int32);
        let cuenta = AtomicUsize::new(0);
        let buenos: [*const c_void; 2] = [ptr::null(), datos.as_ptr().cast()];

        // Longitud u offset negativos.
        for (length, offset) in [(-1, 0), (3, -1)] {
            let mut p = buenos;
            let a = hoja(length, 0, offset, &mut p, &cuenta);
            // SAFETY: nunca llega a leer: falla al validar los numeros.
            assert!(matches!(unsafe { column_of(&s, &a) }, Err(Error::Ffi(_))));
        }
        // null_count por debajo de -1.
        let mut p = buenos;
        let a = hoja(3, -2, 0, &mut p, &cuenta);
        // SAFETY: falla antes de leer.
        assert!(matches!(unsafe { column_of(&s, &a) }, Err(Error::Ffi(_))));
        // Cantidad de buffers equivocada para el tipo.
        let mut un_buffer = [ptr::null()];
        let a = hoja(3, 0, 0, &mut un_buffer, &cuenta);
        // SAFETY: falla antes de leer.
        assert!(matches!(unsafe { column_of(&s, &a) }, Err(Error::Ffi(_))));
        // Buffer de valores NULL pero se esperan 12 bytes.
        let mut sin_valores: [*const c_void; 2] = [ptr::null(), ptr::null()];
        let a = hoja(3, 0, 0, &mut sin_valores, &cuenta);
        // SAFETY: falla antes de leer: el puntero es NULL.
        assert!(matches!(unsafe { column_of(&s, &a) }, Err(Error::Ffi(_))));
        // Array que ya habia sido liberado.
        let mut p = buenos;
        let mut a = hoja(3, 0, 0, &mut p, &cuenta);
        a.release = None;
        // SAFETY: falla antes de leer.
        assert!(matches!(unsafe { column_of(&s, &a) }, Err(Error::Ffi(_))));
        liberar_schema(s);
    }

    #[test]
    fn utf8_con_offsets_decrecientes_es_ffi() {
        let offsets = [0_i32, 3, 1];
        let datos = *b"abc";
        let mut ptrs: [*const c_void; 3] =
            [ptr::null(), offsets.as_ptr().cast(), datos.as_ptr().cast()];
        let cuenta = AtomicUsize::new(0);
        let a = hoja(2, 0, 0, &mut ptrs, &cuenta);
        let s = esquema_hoja(DataType::Utf8);
        // SAFETY: offsets cubre 3 valores y los datos los 3 bytes del ultimo offset.
        let r = unsafe { column_of(&s, &a) };
        assert!(matches!(r, Err(Error::Ffi(_))));
        liberar_schema(s);
    }

    #[test]
    fn utf8_vacio_puede_venir_sin_offsets() {
        let mut ptrs: [*const c_void; 3] = [ptr::null(); 3];
        let cuenta = AtomicUsize::new(0);
        let a = hoja(0, 0, 0, &mut ptrs, &cuenta);
        let s = esquema_hoja(DataType::Utf8);
        // SAFETY: con length 0 no se lee nada.
        let c = unsafe { column_of(&s, &a) }.unwrap();
        assert!(c.is_empty());
        liberar_schema(s);
    }

    #[test]
    fn formato_no_soportado_es_unsupported_y_no_ffi() {
        let mut s = esquema_hoja(DataType::Int32);
        s.format = c"tdD".as_ptr(); // fecha: existe en Arrow, geoda aun no la maneja
        let mut ptrs: [*const c_void; 2] = [ptr::null(); 2];
        let cuenta = AtomicUsize::new(0);
        let a = hoja(0, 0, 0, &mut ptrs, &cuenta);
        // SAFETY: falla antes de leer.
        assert!(matches!(
            unsafe { column_of(&s, &a) },
            Err(Error::Unsupported(_))
        ));
        s.format = c"i".as_ptr();
        liberar_schema(s);
    }

    #[test]
    fn importar_un_esquema() {
        let schema = Schema::try_new(vec![
            Field::new("a", DataType::Float64, false),
            Field::new("b", DataType::Utf8, true),
        ])
        .unwrap();
        let s = export_schema(&schema).unwrap();
        // SAFETY: lo acabamos de exportar nosotros.
        assert_eq!(unsafe { import_schema(&s) }.unwrap(), schema);
        liberar_schema(s);
    }

    #[test]
    fn esquema_que_no_es_struct_es_unsupported() {
        let s = esquema_hoja(DataType::Int32);
        // SAFETY: lo acabamos de exportar nosotros.
        assert!(matches!(
            unsafe { import_schema(&s) },
            Err(Error::Unsupported(_))
        ));
        liberar_schema(s);
    }

    // Estos dos tests protegen el contrato con C. Si alguien reordena o cambia un
    // campo, el tamano cambia y el test avisa antes de que el otro lado lea mal.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn arrow_schema_mide_72_bytes() {
        assert_eq!(std::mem::size_of::<ArrowSchema>(), 72);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn arrow_array_mide_80_bytes() {
        assert_eq!(std::mem::size_of::<ArrowArray>(), 80);
    }
}
