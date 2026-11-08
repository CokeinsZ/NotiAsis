"""Videos de producto para la plantilla video_producto.

Los videos viven en el volumen de media del servidor:

    {MEDIA_DIR}/business/{business_id}/products/*.mp4

Relación producto -> video, en este orden:

1. `{product_id}.mp4`: el id de producto de Dropi. Sirve para amarrar un
   producto a un video cuando el nombre no basta; puede ser un symlink
   relativo al video (`ln -s CAYENNE_PEPPER.mp4 2240718.mp4`), que se
   sube a WhatsApp una sola vez.
2. Por nombre: el archivo corresponde al producto si TODAS las palabras
   del nombre del archivo aparecen en el nombre del producto. Se ignoran
   mayúsculas, tildes, signos y conectores ("de", "x", "the"...), y una
   palabra de 4 letras o más también vale dentro de otra ("club" en
   "CLUBTHE"). Si varios archivos sirven, gana el más específico (el de
   más palabras).

       ASAD_CLUB_DE_NUIT.mp4 -> "ASAD-CLUB DE NUIT", "COMBO ASAD X CLUB DE NUIT",
                                "PERFUMES ASAD+CLUBTHE NUITFULL+PERFUMERO"
       CAYENNE_PEPPER.mp4    -> "CAYENNE PEPPER PRIV"

Subir un video a WhatsApp devuelve un media_id que Meta conserva 30 días.
Se guarda en un caché en disco y se reutiliza en todos los envíos hasta
poco antes de vencer, o hasta que el archivo cambie.
"""

import json
import os
import re
import threading
import time
import unicodedata
from pathlib import Path
from typing import Callable

from app.core.interfaces import MediaUploader, ProductVideoSource

VIDEO_EXTENSION = ".mp4"
VIDEO_MIME_TYPE = "video/mp4"
# Límite de Meta para videos.
MAX_VIDEO_BYTES = 16 * 1024 * 1024
# Meta conserva el media 30 días; se vuelve a subir un poco antes.
MEDIA_TTL_SECONDS = 28 * 24 * 60 * 60

# Palabras que no distinguen un producto de otro.
CONNECTORS = frozenset({"a", "and", "con", "de", "del", "el", "en", "la", "las", "los", "of", "para", "the", "x", "y"})
# Desde este largo una palabra del archivo vale dentro de otra del producto.
MIN_PARTIAL_WORD = 4


class ProductVideoLibrary(ProductVideoSource):
    def __init__(
        self,
        media_dir: str | Path,
        uploader: MediaUploader,
        cache_path: str | Path,
        namespace: str = "",
        clock: Callable[[], float] = time.time,
    ) -> None:
        self._media_dir = Path(media_dir)
        self._uploader = uploader
        self._cache_path = Path(cache_path)
        # Un media_id solo sirve para el número que lo subió.
        self._namespace = namespace
        self._clock = clock
        self._lock = threading.Lock()
        self._cache: dict[str, dict] | None = None

    def video_for(self, business_id: int, products: list[dict]) -> tuple[str, str] | None:
        path = self.find_video(business_id, products)
        if path is None:
            return None
        media_id = self._media_id(path)
        return (media_id, path.name) if media_id else None

    def discard(self, media_id: str) -> None:
        with self._lock:
            cache = self._load()
            stale = [key for key, entry in cache.items() if entry.get("media_id") == media_id]
            for key in stale:
                del cache[key]
            if stale:
                self._save(cache)

    def find_video(self, business_id: int, products: list[dict]) -> Path | None:
        """Video del primer producto que tenga uno."""
        folder = self._media_dir / "business" / str(business_id) / "products"
        try:
            videos = sorted(
                p for p in folder.iterdir() if p.is_file() and p.suffix.lower() == VIDEO_EXTENSION
            )
        except OSError:
            return None

        by_product_id = {p.stem: p for p in videos if p.stem.isdigit()}
        by_name = [(p, words) for p in videos if not p.stem.isdigit() and (words := _words(p.stem))]

        for product in products:
            product_id = product.get("product_id")
            if product_id is not None and str(product_id) in by_product_id:
                return by_product_id[str(product_id)]

            product_words = _words(product.get("product_name") or "")
            matches = [
                (p, words) for p, words in by_name
                if all(_word_in(word, product_words) for word in words)
            ]
            if matches:
                # El más específico; a igualdad, orden alfabético (determinista).
                return min(matches, key=lambda m: (-len(m[1]), -sum(map(len, m[1])), m[0].name))[0]
        return None

    def _media_id(self, path: Path) -> str | None:
        try:
            stat = path.stat()
        except OSError as e:
            print(f"Video {path.name} no disponible: {e}")
            return None
        if stat.st_size > MAX_VIDEO_BYTES:
            print(f"Video {path.name} pesa {stat.st_size / 1024 / 1024:.1f} MB: WhatsApp acepta máximo 16 MB")
            return None

        # Por el archivo real: los symlinks ({product_id}.mp4 -> VIDEO.mp4)
        # comparten una sola subida.
        path = path.resolve()
        key = f"{self._namespace}:{path}"
        with self._lock:
            cache = self._load()
            entry = cache.get(key)
            if (
                entry
                and entry.get("size") == stat.st_size
                and entry.get("mtime_ns") == stat.st_mtime_ns
                and self._clock() - entry.get("uploaded_at", 0) < MEDIA_TTL_SECONDS
            ):
                return entry["media_id"]

            # Subir dentro del lock evita subir el mismo video dos veces si
            # coinciden dos corridas.
            media_id = self._uploader.upload_media(path.read_bytes(), path.name, mime_type=VIDEO_MIME_TYPE)
            if not media_id:
                return None
            cache[key] = {
                "media_id": media_id,
                "uploaded_at": self._clock(),
                "size": stat.st_size,
                "mtime_ns": stat.st_mtime_ns,
            }
            self._save(cache)
            print(f"Video {path.name} subido a WhatsApp ({media_id})")
            return media_id

    def _load(self) -> dict[str, dict]:
        if self._cache is None:
            try:
                self._cache = json.loads(self._cache_path.read_text())
            except (OSError, ValueError):
                self._cache = {}
        return self._cache

    def _save(self, cache: dict[str, dict]) -> None:
        # Escritura atómica; si falla, el caché sigue en memoria hasta que
        # el bot se reinicie.
        try:
            self._cache_path.parent.mkdir(parents=True, exist_ok=True)
            tmp = self._cache_path.with_suffix(".tmp")
            tmp.write_text(json.dumps(cache, indent=2))
            os.replace(tmp, self._cache_path)
        except OSError as e:
            print(f"WARNING: could not save media cache {self._cache_path}: {e}")


def _words(text: str) -> list[str]:
    """Palabras significativas, sin mayúsculas, tildes ni signos."""
    plain = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode().lower()
    return [w for w in re.split(r"[^a-z0-9]+", plain) if w and w not in CONNECTORS]


def _word_in(word: str, product_words: list[str]) -> bool:
    if len(word) >= MIN_PARTIAL_WORD:
        return any(word in candidate for candidate in product_words)
    return word in product_words
