import json

from openai import OpenAI

from app.core.interfaces import RecipientInfoExtractor
from app.models.shipping import RecipientInfo


class DeepSeekRecipientExtractor(RecipientInfoExtractor):
    """Extrae los datos del destinatario de una guía de envío usando DeepSeek."""

    SYSTEM_PROMPT = """
        You are an assistant that extracts the recipient's information and shipping details from shipping labels (guías de envío).
        The text provided contains both sender (quien envía) and recipient (quien recibe) information.
        Your task is to extract the following:
        1. RECIPIENT'S name (destinatario/quien recibe el pedido).
        2. RECIPIENT'S mobile number. Format it so it starts with the country code '+57' followed by the 10-digit number (e.g., +573001234567).
        3. Tracking Number (Número de Guía).
        4. Delivery Address (Lugar al que llega el pedido / Dirección del destinatario o sucursal).
        5. Product (Nomobre resumido del producto que compró el destinatario) Extract only the few keywords from the product (e.g., "ID(2082706)ASAD+CLUB+PERFUMERO  X 1" as "Perfumero") (e.g., "Collar girasol con caja de lujo" as "Collar Girasol")

        If you cannot find a specific piece of information, return an empty string for that field.

        Output strictly in valid JSON format:
        {
          "name": "extracted name",
          "phone": "+57xxxxxxxxxx",
          "tracking_number": "extracted tracking number",
          "delivery_address": "extracted delivery address",
          "product": "extracted product"
        }
        """

    def __init__(
        self,
        api_key: str,
        base_url: str = "https://api.deepseek.com",
        model: str = "deepseek-chat",
    ) -> None:
        self._model = model
        self._client = OpenAI(api_key=api_key or "dummy", base_url=base_url)

    def extract(self, text: str) -> RecipientInfo | None:
        try:
            response = self._client.chat.completions.create(
                model=self._model,
                messages=[
                    {"role": "system", "content": self.SYSTEM_PROMPT},
                    {"role": "user", "content": f"Text to extract from:\n\n{text}"},
                ],
                response_format={"type": "json_object"},
                temperature=0.0,
            )
            result_text = response.choices[0].message.content
            return RecipientInfo.from_dict(json.loads(result_text))
        except Exception as e:
            print(f"Error calling DeepSeek API: {e}")
            return None
