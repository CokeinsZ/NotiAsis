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
        You are an assistant that extracts the recipient's information from shipping labels (guías de envío).
        The text provided contains both sender (quien envía) and recipient (quien recibe) information.
        Your task is to extract ONLY the RECIPIENT'S (destinatario/quien recibe el pedido) name and mobile number.
        Format the mobile number so it starts with the country code '57' followed by the 10-digit number (e.g., 573001234567).
        If you cannot find the recipient information, return empty strings.

        Output strictly in valid JSON format:
        {
        "name": "extracted name",
        "phone": "57xxxxxxxxxx"
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
