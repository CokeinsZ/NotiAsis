import requests
from config import WHATSAPP_TOKEN, WHATSAPP_PHONE_ID

GRAPH_API_URL = "https://graph.facebook.com/v19.0"

def get_media_url(media_id: str) -> str:
    """
    Gets the URL for a media ID from Meta API.
    """
    url = f"{GRAPH_API_URL}/{media_id}"
    headers = {
        "Authorization": f"Bearer {WHATSAPP_TOKEN}"
    }
    response = requests.get(url, headers=headers)
    if response.status_code == 200:
        return response.json().get("url")
    else:
        print(f"Error getting media URL: {response.text}")
        return None

def download_media(media_url: str) -> bytes:
    """
    Downloads the media content from a given Meta media URL.
    """
    headers = {
        "Authorization": f"Bearer {WHATSAPP_TOKEN}"
    }
    response = requests.get(media_url, headers=headers)
    if response.status_code == 200:
        return response.content
    else:
        print(f"Error downloading media: {response.text}")
        return None

def send_whatsapp_message(to_number: str, media_id: str, name: str):
    """
    Sends a WhatsApp message with the document and caption.
    """
    url = f"{GRAPH_API_URL}/{WHATSAPP_PHONE_ID}/messages"
    headers = {
        "Authorization": f"Bearer {WHATSAPP_TOKEN}",
        "Content-Type": "application/json"
    }
    
    caption = f"Hola {name} \nTu pedido ya fue enviado, estate atenta/o para su llegada. ¡Qué lo disfrutes!"
    
    data = {
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to_number,
        "type": "document",
        "document": {
            "id": media_id,
            "caption": caption,
            "filename": "Guia_de_envio.pdf"
        }
    }
    
    response = requests.post(url, headers=headers, json=data)
    if response.status_code == 200:
        print("Message sent successfully")
    else:
        print(f"Error sending message: {response.text}")
