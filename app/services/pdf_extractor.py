from io import BytesIO

import PyPDF2

from app.core.interfaces import TextExtractor


class PdfTextExtractor(TextExtractor):
    """Extrae el texto de documentos PDF recibidos como bytes."""

    def extract_text(self, data: bytes) -> str:
        try:
            reader = PyPDF2.PdfReader(BytesIO(data))
            pages = [page.extract_text() or "" for page in reader.pages]
            return "\n".join(page for page in pages if page)
        except Exception as e:
            print(f"Error extracting PDF text: {e}")
            return ""
