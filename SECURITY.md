# Política de seguridad

## Cómo reportar una vulnerabilidad

**No abras un issue público.** Usa el reporte privado de GitHub:

1. Entra en la pestaña **Security** del repositorio.
2. Pulsa **Report a vulnerability** (o ve a `https://github.com/ElYares/geoda/security/advisories/new`).
3. Describe el problema, la versión afectada y, si puedes, una entrada mínima que lo reproduzca.

Solo la persona que mantiene el proyecto ve ese reporte.

## Qué esperar

| Paso | Plazo objetivo |
|---|---|
| Acuse de recibo | 7 días |
| Primera valoración (si se confirma y qué gravedad tiene) | 14 días |
| Arreglo o plan de arreglo | 90 días como máximo |

Es un proyecto mantenido por una sola persona: los plazos son objetivos, no un contrato. Si no recibes
respuesta en 7 días, vuelve a escribir en el mismo reporte.

Te pedimos que no publiques el problema hasta que haya un arreglo o hayan pasado 90 días, lo que ocurra primero.
Si lo deseas, te mencionamos en el aviso al publicar el arreglo.

## Qué versiones reciben arreglos

geoda aún no ha publicado ninguna versión en un registro (PyPI, npm, Maven Central o crates.io). Mientras
tanto solo se arregla la rama `main`. Cuando haya versiones publicadas, esta tabla dirá cuáles reciben arreglos.

| Versión | Soportada |
|---|---|
| `main` | sí |

## Qué cuenta como vulnerabilidad

geoda procesa datos que llegan por una frontera C, así que nos interesan sobre todo:

- Errores de memoria al importar columnas, por ejemplo desde Arrow (lecturas o escrituras fuera de rango, uso
  tras liberar, doble liberación, desbordamientos de tamaño).
- Una función de la C-API que aborta el proceso con una entrada que debería devolver un error.
- Un archivo de columna mal formado que cause lecturas fuera de rango.
- Credenciales o rutas personales en el repositorio o en los paquetes construidos.

No cuentan las diferencias numéricas dentro de la tolerancia documentada ni el rendimiento con entradas muy
grandes, aunque puedes abrir un issue normal para ellas.

## Sobre el código `unsafe`

El `unsafe` del proyecto se concentra en `memory/`, `interop/arrow_c.rs` y la C-API. Se ha auditado con ayuda de
un agente (no por una persona externa) y no se ha sometido todavía a *fuzzing*. Contamos con que aún haya fallos;
por eso este reporte importa.

El proyecto se prueba con Miri y con ASan/UBSan en la CI. Eso reduce el riesgo, pero no lo elimina.
