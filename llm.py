import json
from openai import OpenAI
from config import DEEPSEEK_API_KEY

client = OpenAI(
    api_key=DEEPSEEK_API_KEY or "dummy",
    base_url="https://api.deepseek.com"
)

def extract_recipient_info(text: str) -> dict:
    """
    Sends text to DeepSeek to extract the recipient's name and phone number.
    Returns a dictionary like {"name": "...", "phone": "57..."} or None if failed.
    """

    system_prompt = """
        You are an assistant that extracts the recipient's information and shipping details from shipping labels (guías de envío).
        The text provided contains both sender (quien envía) and recipient (quien recibe) information.
        Your task is to extract the following:
        1. RECIPIENT'S name (destinatario/quien recibe el pedido).
        2. RECIPIENT'S mobile number. Format it so it starts with the country code '+57' followed by the 10-digit number (e.g., +573001234567).
        3. Tracking Number (Número de Guía).
        4. Delivery Address (Lugar al que llega el pedido / Dirección del destinatario o sucursal).

        If you cannot find a specific piece of information, return an empty string for that field.

        Output strictly in valid JSON format:
        {
          "name": "extracted name",
          "phone": "+57xxxxxxxxxx",
          "tracking_number": "extracted tracking number",
          "delivery_address": "extracted delivery address"
        }
        """
    
    try:
        response = client.chat.completions.create(
            model="deepseek-chat",
            messages=[
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": f"Text to extract from:\n\n{text}"}
            ],
            response_format={"type": "json_object"},
            temperature=0.0
        )
        
        result_text = response.choices[0].message.content
        return json.loads(result_text)
    except Exception as e:
        print(f"Error calling DeepSeek API: {e}")
        return None
