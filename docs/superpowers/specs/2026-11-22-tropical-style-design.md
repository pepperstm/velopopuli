# Estilo tropical fiel al camino — diseño

Fecha: 2026-11-22 · Rama: `feat/tropical-style` · Estado: borrador para aprobar

## Intención y por qué

El usuario rueda en rodillo rutas que conoce en Samborondón / Guayaquil (Ecuador,
~2° S, casi a nivel del mar). La primera ruta es Plaza Lagos → Malecón de
Samborondón (28 km por Av. Samborondón, Vía a Salitre y Vía Samborondón; GPX en
`~/Downloads/plaza-lagos-samborondon.gpx`). Hoy VeloPopuli la arma con árboles
europeos (abetos, pinos, abedules), vallas de carrera a ambos lados y sin nada de lo
que caracteriza esa avenida. El usuario quiere **reconocer el camino**: el estilo
de allá (palmeras, parterre central con plantas, postes de luz, torres de alta
tensión, ciclovía verde, muros claros de urbanizaciones, centros comerciales) y que
lo que de verdad está en el camino aparezca **donde está**.

Decisión del usuario: "que se parezca" (estilo por reglas) con fidelidad a los datos
reales donde existan. Enfoque elegido: **A. Estilo tropical + datos reales del mapa**.

Referencia visual: captura de Street View de Av. Samborondón (abril 2025) que mandó
el usuario. Sirve **solo como referencia de estilo**; no se extrae ningún dato de
Google (sus condiciones lo prohíben).

## Objetivos

1. **Vegetación tropical.** Uno o más tipos nuevos de árbol con forma de palmera, y
   una mezcla de especies tropical que reemplaza la europea en tierras bajas del
   trópico. Se activa sola según latitud y altura; fuera del trópico nada cambia.
2. **Vía abierta** (opcional, ajuste del usuario). En vez de vallas de carrera, el
   borde real de una avenida: bordillo, vereda de hormigón y franja de césped.
3. **Parterre central.** Cuando la ruta va por una calzada de una vía doble, el
   espacio entre las dos calzadas se ve como parterre: césped, fila de palmeras y
   arbustos bajos.
4. **Postes de luz.** Donde OpenStreetMap tiene `highway=street_lamp`, en su sitio
   exacto. Donde no hay datos, por regla en vías principales de zona urbana: poste
   de doble brazo en el parterre de las vías dobles, de un brazo en el borde en las
   demás, cada ~35–40 m.
5. **Líneas eléctricas.** `power=line` / `power=minor_line` con sus nodos
   `power=tower` / `power=pole` de OpenStreetMap: torres o postes en su sitio real y
   cables con catenaria entre ellos. En los primeros 6 km de la ruta de prueba hay
   73 torres mapeadas.
6. **Ciclovía.** Franja verde pintada en el borde de la calzada cuando la vía de la
   ruta tiene `cycleway`, `cycleway:right|left|both` = `lane` (o `track`) en OSM.
7. **Muros de urbanización.** Los muros mapeados (`barrier=wall`) ya se dibujan; se
   añade una regla para cuando faltan: en el trópico, el borde de un
   `landuse=residential` que da a una vía principal lleva muro claro.

8. **Líneas de la calzada al estilo de América.** Línea central amarilla (doble en
   vías rurales de dos carriles), bordes blancos. Hoy el motor pinta la línea central
   blanca, al estilo europeo.
9. **Separador de hormigón** (muro tipo New Jersey) en el centro de las vías dobles
   fuera de la zona urbana, en vez del parterre con plantas. Desde OSM
   (`barrier=jersey_barrier`) si está mapeado; si no, por regla.
10. **Árbol tropical de copa ancha** (tipo samán / ficus) además de las palmeras: es
    el árbol dominante en el tramo rural y en la entrada del pueblo.

Todo lo que viene de datos reales gana sobre la regla en ese tramo. Los datos de OSM
de los objetivos 4, 5, 6, 9 y 10 llegan por el script de preparación (ver "Decisiones
mías"); si alguien mapea después palmeras, postes o la ciclovía en OSM, aparecen al
volver a correr el script, sin cambiar código.

## Guía de estilo por tipo de tramo

Sacada de mirar 5 puntos de la ruta en Street View (más la captura del usuario),
**solo como referencia de cómo se ve cada tipo de tramo**. No se copia ninguna
posición ni objeto concreto de Google. **Qué tipo de tramo es cada parte de la ruta lo
decide el motor con datos de OSM** (clase de vía, carriles, `oneway`, edificios y
`landuse` cercanos), no con Street View.

| Tipo de tramo | Cómo se reconoce en OSM | Cómo debe verse |
|---|---|---|
| **Avenida urbana** (Av. Samborondón, km 0–6) | `trunk`/`primary`, vía doble (`oneway` + calzada contraria), 3–4 carriles, edificios o `landuse` residencial/comercial cerca | Parterre con bordillo pintado de amarillo, césped, palmeras altas y arbustos; postes altos grises de doble brazo en el parterre; bordillo y vereda de hormigón; ciclovía verde donde OSM la marque; muros crema de urbanización; plazas comerciales bajas blancas con techo metálico gris y estacionamiento adoquinado; pasarelas peatonales metálicas (ya salen como `overbridges` si están mapeadas) |
| **Vía periurbana** (Vía a Salitre / Vía a Samborondón, km 6–12) | `trunk`/`primary`, vía doble de 2 carriles, pocos edificios | Separador central de hormigón tipo New Jersey; postes de hormigón con muchos cables a ambos lados; bermas de grava; galpones bajos; maleza y árboles tropicales de copa ancha |
| **Carretera rural** (Guayas 2, km 12–25) | una sola calzada de 2 carriles, sin edificios cerca, `farmland`/`wetland`/agua alrededor | Doble línea central amarilla y bordes blancos; vía sobre terraplén con taludes de césped entre campos planos con agua (arrozales) cuando OSM marque agua o humedal; postes blancos bajos al borde de la berma; árboles de copa ancha dispersos; postes eléctricos de madera u hormigón; sin luminarias |
| **Entrada al pueblo** (km 25–28) | calles con edificios bajos, `landuse=residential` | Línea central amarilla; bermas anchas de tierra; árboles grandes de copa paraguas; casas bajas y galpones; postes con brazo de luz; tanque de agua elevado si OSM lo tiene (`man_made=water_tower`) |

En toda la ruta: terreno completamente plano, verdes intensos, cielo a menudo
nublado y con bruma (se puede elegir con los ajustes de clima que ya existen).

## Fase 2: datos abiertos de satélite y fotos de calle

El usuario pidió usar también Mapillary e imágenes satelitales. La fase 1 (objetivos
1–10) no depende de esto; la fase 2 sube la fidelidad poniendo más cosas **donde de
verdad están**, solo con fuentes de licencia abierta:

| Qué | Fuente | Licencia | Para qué |
|---|---|---|---|
| **Edificios** | Google Open Buildings v3 (detectados en imágenes satelitales; cubre Ecuador) | CC BY 4.0 u ODbL | Rellenar las casas de las urbanizaciones que faltan en OSM. OSM gana: un edificio de Google solo entra si no se superpone con uno de OSM |
| **Árboles** | Meta / WRI, mapa global de altura de copas a 1 m (de imágenes Maxar) | CC BY 4.0 | Árboles en su sitio real y con su altura; la especie (palmera, samán, arbusto) la sigue eligiendo la regla tropical según la forma de la copa |
| **Postes y luminarias** | Mapillary, objetos detectados (`object--street-light`, `object--support--utility-pole`) | Datos de Mapillary (confirmar licencia antes de usar) | Postes en su sitio en los tramos con cobertura (Av. Samborondón); necesita una clave gratis del usuario, que la app ya sabe pedir |
| **Suelo** | ESA WorldCover 10 m | CC BY 4.0 | Agua, humedal y cultivo donde OSM no los marca (arrozales del tramo rural) |

**Decisión mía (corregible): un script de preparación por ruta.** Un script aparte
(Python con `uv`) descarga solo el corredor de la ruta (±500 m) de esas fuentes y
escribe un paquete compacto en la carpeta de datos de la app
(`data/opendata-<route id>.json`). El motor lo usa si existe y, si no, todo funciona
como en la fase 1. Así no hay que leer GeoParquet ni GeoTIFF dentro de la app en Rust.
Integrarlo dentro de la app (que se haga solo al importar una ruta) queda para
después, si vale la pena.

**Atribución:** cada fuente CC BY se acredita en la pantalla de créditos de la app y
en el README, como ya se hace con OpenStreetMap.

## No objetivos

- Copiar o extraer nada de Google Maps / Street View / Google Earth (sí se usa el
  conjunto **abierto** Google Open Buildings, que Google publica con licencia libre).
- Colocar cada objeto en su sitio exacto donde no hay datos abiertos (eso sería el
  enfoque B o C, que quedan para después).
- Tráfico (autos, buses), señales de tránsito, paradas de bus.
- Foto satelital como textura del suelo. La única abierta para Ecuador es Sentinel-2
  de 10 m (borrosa); está en la rama `next` del proyecto original, que choca con
  `main`. Queda para cuando esa rama se una.
- Estilo regional de edificios (colores y techos). La rama `next` ya trae
  `world/style.rs` para eso; no duplicarlo aquí.
- Paridad completa con el renderizador nativo (opcional y no es el predeterminado).

## Anti-objetivos (sería un fracaso aunque funcione)

- Que cambie el aspecto de rutas fuera del trópico (Alpes, Mallorca, Devon). Las
  reglas tropicales solo se activan en el trópico, y la vía abierta solo si el usuario
  la elige.
- Que una ruta falle o se quede sin estilo cuando Overpass no responde. Las reglas
  (palmeras, parterre, postes por regla) no dependen de Overpass.
- Que baje notablemente la fluidez: palmeras y postes deben dibujarse con instancing,
  como los árboles actuales.
- Que un objeto puesto por regla tape o desplace a uno real. Lo real siempre queda en
  su sitio; la regla solo rellena huecos.

## Restricciones

- El mundo se construye en Rust (`crates/core/src/world`); la página (three.js) solo
  sube los buffers que recibe. Seguir ese reparto.
- Árboles: `world/trees.rs` (modelos low-poly, `KINDS`, `Climate::kind`), dibujados
  con instancing y sprites desde 200 m (`web/src/world/trees.ts`). Un tipo nuevo de
  árbol hereda automáticamente sprites y atlas.
- Datos de OSM detallados en la app: `world/surface.rs` (`way_query`) hace una
  consulta Overpass por cada tramo de 20 km de ruta, solo ±15 m de la vía
  (`SURFACE_RADIUS_M`) para las vías y ±120 m para muros/cercos, solo líneas (no
  nodos); tras un fallo reintenta como mucho una vez al día (`src-tauri/src/mapdata.rs`).
  Las vector tiles (Shortbread) no traen líneas eléctricas ni postes.
- Overpass es poco fiable: en la prueba de Samborondón ninguno de los tres servidores
  respondió a tiempo ("road tags unavailable"). **No se toca la consulta Overpass de
  la app** (ni `WAY_TAGS`): agrandarla haría que falle más y se perderían también los
  datos de superficie y ancho que hoy funcionan. Los datos de OSM nuevos los trae el
  script de preparación (ver abajo).
- Licencia GPL-3. Mantener los cambios en módulos nuevos donde se pueda, para que
  sea fácil seguir actualizando desde el proyecto original y, si el usuario quiere,
  proponerlos como pull request.
- Tests en `crates/core/tests`; verificación visual con el smoke harness
  (`VP_SMOKE`, `VP_SMOKE_ROUTE`, `VP_SMOKE_AT`, `VP_SMOKE_WINDOW`).

## Enfoques considerados

| Enfoque | Qué es | A favor | En contra |
|---|---|---|---|
| **A. Estilo tropical + datos reales** (elegido) | Reglas tropicales en el motor + todo lo que OSM tenga, en su sitio | Sirve para cualquier ruta tropical; mejora sola cuando OSM mejora | Donde no hay datos, la posición es aproximada |
| B. Archivo por ruta | Lista hecha a mano de objetos por km para esta ruta | Lo más fiel para una ruta | Trabajo manual por ruta; formato nuevo |
| C. Completar OSM primero | Mapear palmeras/postes/ciclovía desde Mapillary y luego dibujarlos | Fiel y beneficia a todos | Horas de edición pública antes de ver algo |

El usuario eligió A. B y C quedan como mejoras posteriores para tramos concretos.

## Lo que el usuario decidió

- Estilo "que se parezca", fiel a lo real donde haya datos.
- Enfoque A.
- Street View solo como referencia de estilo; nada extraído de Google Maps.
- Usar todas las fuentes abiertas posibles: Mapillary, datos sacados de imágenes
  satelitales con licencia libre, etc. ("hazlo donde puedas").

## Decisiones mías (corregibles)

- **Dónde es "trópico":** latitud absoluta del centro de la ruta < 23,5° y altura
  < ~1.200 m. Por encima (p. ej. Quito, 2.800 m) sigue la mezcla actual. Transición
  suave en los bordes, igual que las demás mezclas de `Climate`.
- **Tipos de palmera:** al menos una alta y fina (tipo cocotero / palma real) y una
  baja y abierta (tipo palma de jardín). Hechas con la misma técnica low-poly del
  resto de árboles (tronco en segmentos + hojas como abanicos de triángulos).
- **Vía abierta como ajuste**, con las vallas de carrera como predeterminado, para no
  cambiar el comportamiento del proyecto original. El usuario lo activa una vez.
- **Ciclovía solo desde OSM**, sin regla: poner ciclovías por regla en todas las vías
  grandes del trópico sería falso en la mayoría de los casos.
- **Detección de vía doble:** la ruta va por una vía `oneway` y hay otra calzada
  `oneway` de sentido contrario casi paralela a pocos metros (ya se reconocen al
  dibujar "otras vías" en `world/scenery.rs`). El ancho del parterre sale de la
  distancia entre ambas.
- **Postes por regla** solo en zonas urbanas (cerca de edificios o `landuse`
  residencial/comercial), para no iluminar la vía rural entera.
- **Renderizador nativo:** recibe las palmeras (comparte modelos y atlas); postes,
  cables, bordillos y ciclovía pueden faltar en v1.
- **Alcance geográfico:** en esta versión todo lo nuevo (reglas y datos nuevos) se
  activa solo en el trópico; las líneas amarillas, en América. Fuera de eso, Sa Calobra
  y cualquier ruta europea se ven exactamente igual que hoy. Extenderlo al resto del
  mundo (p. ej. líneas eléctricas en los Alpes) queda para después, detrás de un ajuste.
- **De dónde salen los datos de OSM nuevos:** líneas eléctricas con sus torres y
  postes, luminarias mapeadas, ciclovías, separadores New Jersey y tanques de agua
  **no** se piden en la consulta Overpass de la app. Los trae el **script de
  preparación por ruta** (el mismo de la fase 2), con consultas separadas, más tiempo
  y reintentos, y los guarda en el paquete de datos de la ruta. Consecuencia: esas
  cosas aparecen en rutas preparadas con el script; las reglas (palmeras, parterre,
  postes por regla, vía abierta, líneas amarillas) funcionan en cualquier ruta
  tropical sin script. Si alguien mapea más cosas en OSM, aparecen al volver a correr
  el script.
- **Huecos entre datos reales:** donde OSM tiene solo algunos postes (p. ej. 3 en
  2 km), los postes por regla rellenan los huecos, alineados con los reales, nunca a
  menos de medio espaciado de uno real, y con el mismo aspecto. Igual con los muros:
  la regla solo dibuja muro donde no hay uno mapeado cerca.
- **Verificación sin depender de Overpass en vivo:** cada paso se prueba con un
  paquete de datos guardado en el repo de pruebas (respuesta de Overpass grabada), y
  además con capturas en la app real cuando los servidores respondan.

## Orden sugerido de entrega

Fase 1 (reglas, sin script):

1. Palmeras, samán + mezcla tropical (lo que más cambia la primera impresión).
2. Vía abierta (sin vallas, con bordillo y vereda).
3. Líneas amarillas de calzada.
4. Parterre con palmeras + postes por regla; separador de hormigón por regla en vías
   periurbanas.
5. Regla de muros de urbanización.

Fase 1b (datos reales de OSM vía script):

6. Script de preparación por ruta + paquete de datos que el motor lee.
7. Líneas eléctricas, luminarias mapeadas, ciclovía, separadores y tanques de agua
   desde el paquete.

Fase 2 (datos abiertos de satélite y fotos, mismo script):

8. Edificios de Google Open Buildings donde OSM no tiene.
9. Árboles reales desde el mapa de copas de Meta/WRI.
10. Postes y luminarias de Mapillary (con la clave gratis del usuario).
11. Suelo de ESA WorldCover (agua, humedal, cultivo).

Fuera de esta versión (posibles mejoras después): postes blancos delineadores,
garitas en la entrada de urbanizaciones, vallas publicitarias, arrozales con agua
donde OSM no los marca.

Cada paso se verifica con capturas del smoke harness en Plaza Lagos → Samborondón
(km 1, 3 y 15) y en Sa Calobra (para confirmar que fuera del trópico no cambia).

## Lo que queda para quien lo construya

- Formas exactas, colores y tamaños de palmeras, postes, torres y bordillos.
- Espaciado fino y variación aleatoria (semillas por posición, como el resto).
- Estructura de datos para postes y cables y cómo llegan a la página.
- Cómo se reparte el trabajo entre `trees.rs`, `barriers.rs`, `scenery.rs` y
  módulos nuevos.
- Tests concretos y umbrales de rendimiento.
