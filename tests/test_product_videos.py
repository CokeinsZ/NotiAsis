"""Tests de los videos de producto: relación producto -> archivo y caché
de media_id de WhatsApp. Usa carpetas temporales y un uploader falso."""

import json
import os

import pytest

from app.services.product_videos import MAX_VIDEO_BYTES, MEDIA_TTL_SECONDS, ProductVideoLibrary

DAY = 24 * 60 * 60


class FakeUploader:
    def __init__(self, fail=False):
        self.uploads = []
        self.fail = fail

    def upload_media(self, data, filename, mime_type="application/pdf"):
        self.uploads.append((filename, mime_type, len(data)))
        return None if self.fail else f"media-{len(self.uploads)}"


class Clock:
    def __init__(self, now=1_000_000.0):
        self.now = now

    def __call__(self):
        return self.now


def make_library(tmp_path, files=("ASAD_CLUB_DE_NUIT.mp4", "CAYENNE_PEPPER.mp4"), business_id=1,
                 uploader=None, clock=None, namespace="phone-1"):
    folder = tmp_path / "media" / "business" / str(business_id) / "products"
    folder.mkdir(parents=True, exist_ok=True)
    for name in files:
        video = folder / name
        if not video.exists():  # reescribirlo cambiaría su mtime: sería otro video
            video.write_bytes(b"video:" + name.encode())
    library = ProductVideoLibrary(
        media_dir=tmp_path / "media",
        uploader=uploader or FakeUploader(),
        cache_path=tmp_path / "state" / "whatsapp_media.json",
        namespace=namespace,
        clock=clock or Clock(),
    )
    return library, folder


def video_name(library, *names, business_id=1):
    path = library.find_video(business_id, [{"product_name": n} for n in names])
    return path.name if path else None


# ------------------------------ Producto -> video ------------------------------


@pytest.mark.parametrize("product,expected", [
    # Nombres reales de guide_products contra los videos del servidor.
    ("ASAD-CLUB DE NUIT", "ASAD_CLUB_DE_NUIT.mp4"),
    ("COMBO ASAD X CLUB DE NUIT", "ASAD_CLUB_DE_NUIT.mp4"),
    ("PERFUMES ASAD+CLUBTHE NUITFULL+PERFUMERO", "ASAD_CLUB_DE_NUIT.mp4"),
    ("CAYENNE PEPPER PRIV", "CAYENNE_PEPPER.mp4"),
    ("CAYENNE PEPPER. CM", "CAYENNE_PEPPER.mp4"),
    # Por nombre no alcanzan: se amarran con un symlink {product_id}.mp4.
    ("EXTRACTO PIMIENTA DE CAYENA 3000 Mg", None),
    ("ASAD+CLUB+PERFUMERO", None),
    ("Collar Girasol GIRATORIO + Caja Corazón", None),
    ("Collar Rosa Roja + CAJA 6ROSAS ROSADA", None),
    ("Collar girasol compra", None),
])
def test_matches_real_product_names(tmp_path, product, expected):
    library, _ = make_library(tmp_path)

    assert video_name(library, product) == expected


def test_matching_ignores_case_accents_and_separators(tmp_path):
    library, _ = make_library(tmp_path, files=("collar-girasol corazón.MP4",))

    assert video_name(library, "COLLAR GIRASOL GIRATORIO + Caja Corazon") == "collar-girasol corazón.MP4"


def test_most_specific_video_wins(tmp_path):
    library, _ = make_library(tmp_path, files=("COLLAR.mp4", "COLLAR_GIRASOL.mp4", "COLLAR_ROSA.mp4"))

    assert video_name(library, "Collar Girasol GIRATORIO") == "COLLAR_GIRASOL.mp4"
    assert video_name(library, "Collar Rosa Roja") == "COLLAR_ROSA.mp4"
    assert video_name(library, "Collar de perlas") == "COLLAR.mp4"


def test_short_words_must_match_whole_word(tmp_path):
    library, _ = make_library(tmp_path, files=("ORO.mp4",))

    assert video_name(library, "PERFUMERO") is None  # "oro" dentro de otra palabra no cuenta
    assert video_name(library, "Collar de ORO 18k") == "ORO.mp4"


def test_product_id_file_wins_over_name(tmp_path):
    library, _ = make_library(tmp_path, files=("CAYENNE_PEPPER.mp4", "4455.mp4"))

    path = library.find_video(1, [{"product_id": 4455, "product_name": "CAYENNE PEPPER PRIV"}])

    assert path.name == "4455.mp4"


SERVER_SYMLINKS = {
    "2240718": "CAYENNE_PEPPER.mp4",  # EXTRACTO PIMIENTA DE CAYENA 3000 Mg
    "2246234": "CAYENNE_PEPPER.mp4",  # CAYENNE PEPPER. CM
    "2268785": "CAYENNE_PEPPER.mp4",  # CAYENNE PEPPER PRIV
    "1951177": "ASAD_CLUB_DE_NUIT.mp4",  # ASAD-CLUB DE NUIT
    "2290274": "ASAD_CLUB_DE_NUIT.mp4",  # COMBO ASAD X CLUB DE NUIT
    "999427": "ASAD_CLUB_DE_NUIT.mp4",  # PERFUMES ASAD+CLUBTHE NUITFULL+PERFUMERO
    "2082706": "ASAD_CLUB_DE_NUIT.mp4",  # ASAD+CLUB+PERFUMERO
}


def test_product_id_symlinks_share_a_single_upload(tmp_path):
    uploader = FakeUploader()
    library, folder = make_library(tmp_path, uploader=uploader)
    for product_id, target in SERVER_SYMLINKS.items():
        (folder / f"{product_id}.mp4").symlink_to(target)  # relativo, como `ln -s` en la carpeta

    media = {
        product_id: library.video_for(1, [{"product_id": int(product_id), "product_name": "?"}])[0]
        for product_id in SERVER_SYMLINKS
    }

    assert media["2240718"] == media["2246234"] == media["2268785"]
    assert media["1951177"] == media["2290274"] == media["999427"] == media["2082706"]
    assert sorted(u[0] for u in uploader.uploads) == ["ASAD_CLUB_DE_NUIT.mp4", "CAYENNE_PEPPER.mp4"]


def test_broken_symlink_is_ignored(tmp_path):
    # Un symlink absoluto del servidor no existe dentro del contenedor.
    library, folder = make_library(tmp_path)
    (folder / "2240718.mp4").symlink_to("/home/debian/vicky-v/media/business/1/products/CAYENNE_PEPPER.mp4")

    products = [{"product_id": 2240718, "product_name": "EXTRACTO PIMIENTA DE CAYENA 3000 Mg"}]

    assert library.find_video(1, products) is None


def test_first_product_with_video_is_used(tmp_path):
    library, _ = make_library(tmp_path)

    assert video_name(library, "Collar girasol compra", "CAYENNE PEPPER PRIV") == "CAYENNE_PEPPER.mp4"


def test_videos_are_per_business_and_only_mp4(tmp_path):
    library, folder = make_library(tmp_path, files=("CAYENNE_PEPPER.mov",))

    assert video_name(library, "CAYENNE PEPPER PRIV") is None
    assert video_name(library, "ASAD-CLUB DE NUIT", business_id=2) is None  # sin carpeta


# ------------------------------ Caché de media_id ------------------------------


def test_video_is_uploaded_once_and_reused(tmp_path):
    uploader = FakeUploader()
    library, _ = make_library(tmp_path, uploader=uploader)
    products = [{"product_name": "ASAD-CLUB DE NUIT"}]

    first = library.video_for(1, products)
    second = library.video_for(1, products)

    assert first == second == ("media-1", "ASAD_CLUB_DE_NUIT.mp4")
    assert uploader.uploads == [("ASAD_CLUB_DE_NUIT.mp4", "video/mp4", len(b"video:ASAD_CLUB_DE_NUIT.mp4"))]


def test_cache_survives_restarts(tmp_path):
    products = [{"product_name": "CAYENNE PEPPER PRIV"}]
    library, _ = make_library(tmp_path)
    library.video_for(1, products)

    uploader = FakeUploader()
    restarted, _ = make_library(tmp_path, uploader=uploader)

    assert restarted.video_for(1, products) == ("media-1", "CAYENNE_PEPPER.mp4")
    assert uploader.uploads == []
    assert json.loads((tmp_path / "state" / "whatsapp_media.json").read_text())


def test_video_is_uploaded_again_before_meta_expires_it(tmp_path):
    uploader, clock = FakeUploader(), Clock()
    library, _ = make_library(tmp_path, uploader=uploader, clock=clock)
    products = [{"product_name": "CAYENNE PEPPER PRIV"}]
    library.video_for(1, products)

    clock.now += MEDIA_TTL_SECONDS - 60
    assert library.video_for(1, products)[0] == "media-1"

    clock.now += 120
    assert library.video_for(1, products)[0] == "media-2"
    assert MEDIA_TTL_SECONDS < 30 * DAY  # Meta borra el media a los 30 días


def test_replaced_file_is_uploaded_again(tmp_path):
    uploader = FakeUploader()
    library, folder = make_library(tmp_path, uploader=uploader)
    products = [{"product_name": "CAYENNE PEPPER PRIV"}]
    library.video_for(1, products)

    video = folder / "CAYENNE_PEPPER.mp4"
    video.write_bytes(b"nuevo video con otro contenido")
    os.utime(video, ns=(1, 1))

    assert library.video_for(1, products)[0] == "media-2"


def test_discarded_media_id_is_uploaded_again(tmp_path):
    uploader = FakeUploader()
    library, _ = make_library(tmp_path, uploader=uploader)
    products = [{"product_name": "CAYENNE PEPPER PRIV"}]
    media_id, _ = library.video_for(1, products)

    library.discard(media_id)

    assert library.video_for(1, products)[0] == "media-2"


def test_media_ids_are_per_phone_number(tmp_path):
    products = [{"product_name": "CAYENNE PEPPER PRIV"}]
    library, _ = make_library(tmp_path, namespace="phone-1")
    library.video_for(1, products)

    uploader = FakeUploader()
    other_number, _ = make_library(tmp_path, uploader=uploader, namespace="phone-2")

    assert other_number.video_for(1, products)[0] == "media-1"
    assert len(uploader.uploads) == 1


def test_failed_upload_is_not_cached(tmp_path):
    products = [{"product_name": "CAYENNE PEPPER PRIV"}]
    library, _ = make_library(tmp_path, uploader=FakeUploader(fail=True))

    assert library.video_for(1, products) is None
    assert library.video_for(1, products) is None
    assert not (tmp_path / "state" / "whatsapp_media.json").exists()


def test_video_over_whatsapp_limit_is_not_uploaded(tmp_path):
    uploader = FakeUploader()
    library, folder = make_library(tmp_path, uploader=uploader)
    with open(folder / "CAYENNE_PEPPER.mp4", "wb") as f:
        f.truncate(MAX_VIDEO_BYTES + 1)

    assert library.video_for(1, [{"product_name": "CAYENNE PEPPER PRIV"}]) is None
    assert uploader.uploads == []


def test_unwritable_cache_still_works_in_memory(tmp_path):
    uploader = FakeUploader()
    library, _ = make_library(tmp_path, uploader=uploader)
    (tmp_path / "state").write_text("no es una carpeta")  # el caché no se puede escribir
    products = [{"product_name": "CAYENNE PEPPER PRIV"}]

    assert library.video_for(1, products)[0] == "media-1"
    assert library.video_for(1, products)[0] == "media-1"
    assert len(uploader.uploads) == 1
