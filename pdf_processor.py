import PyPDF2
from io import BytesIO

def extract_text_from_pdf_bytes(pdf_bytes: bytes) -> str:
    """
    Extract text from a PDF file provided as bytes.
    """
    try:
        reader = PyPDF2.PdfReader(BytesIO(pdf_bytes))
        text = ""
        for page in reader.pages:
            page_text = page.extract_text()
            if page_text:
                text += page_text + "\\n"
        return text
    except Exception as e:
        print(f"Error extracting PDF text: {e}")
        return ""
