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

def send_whatsapp_message(to_number: str, media_id: str, name: str, tracking_number: str, delivery_address: str):
    """
    Sends a WhatsApp template message with a document header and body variables.
    """
    url = f"{GRAPH_API_URL}/{WHATSAPP_PHONE_ID}/messages"
    headers = {
        "Authorization": f"Bearer {WHATSAPP_TOKEN}",
        "Content-Type": "application/json"
    }
    
    template_name = "pedido_eviado"
    to_number = "+573003579384" # Debugging number, replace with the actual recipient number in production
    
    data = {
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to_number,
        "type": "template",
        "template": {
            "name": template_name,
            "language": {
                "code": "es_CO"
            },
            "components": [
                {
                    "type": "header",
                    "parameters": [
                        {
                            "type": "document",
                            "document": {
                                "id": media_id,
                                "filename": "Guia_de_envio.pdf"
                            }
                        }
                    ]
                },
                {
                    "type": "body",
                    "parameters": [
                        {
                            "type": "text",
                            "text": name
                        },
                        {
                            "type": "text",
                            "text": tracking_number
                        },
                        {
                            "type": "text",
                            "text": delivery_address
                        }
                    ]
                }
            ]
        }
    }
    
    response = requests.post(url, headers=headers, json=data)
    if response.status_code == 200:
        print("Message sent successfully")
    else:
        print(f"Error sending message: {response.text}")
