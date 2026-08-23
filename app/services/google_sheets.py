import polars as pl
import requests

from app.core.interfaces import SheetSource


class GoogleSheetClient(SheetSource):
    """Descarga hojas de Google Sheets como DataFrames de polars.

    Usa la URL de exportación CSV pública del documento (el sheet debe
    estar compartido como "cualquiera con el enlace").
    """

    EXPORT_URL = "https://docs.google.com/spreadsheets/d/{document_id}/export?format=csv&gid={gid}"

    def __init__(self, timeout: int = 60) -> None:
        self._timeout = timeout

    def fetch_sheet(self, document_id: str, sheet_gid: str) -> pl.DataFrame:
        url = self.EXPORT_URL.format(document_id=document_id, gid=sheet_gid)
        # numero_guia siempre como texto (los números grandes no deben
        # perder precisión ni formato).
        return pl.read_csv(
            url,
            schema_overrides={"numero_guia": pl.String, "TELEFONO": pl.String},
        )

    def download_file(self, url: str) -> bytes | None:
        try:
            response = requests.get(url, timeout=self._timeout)
            if response.status_code == 200:
                return response.content
            print(f"Error downloading file ({response.status_code}): {url}")
            return None
        except Exception as e:
            print(f"Error downloading file {url}: {e}")
            return None
