from fastapi import FastAPI, Request, Response, BackgroundTasks
from config import VERIFY_TOKEN
from whatsapp import get_media_url, download_media, send_whatsapp_message
from pdf_processor import extract_text_from_pdf_bytes
from llm import extract_recipient_info

app = FastAPI()

def process_webhook_event(data: dict):
    try:
        # Navigate through the WhatsApp webhook payload
        entries = data.get("entry", [])
        for entry in entries:
            changes = entry.get("changes", [])
            for change in changes:
                value = change.get("value", {})
                messages = value.get("messages", [])
                
                for message in messages:
                    # We are only interested in document messages
                    if message.get("type") == "document":
                        document = message.get("document", {})
                        mime_type = document.get("mime_type", "")
                        
                        if "pdf" in mime_type:
                            media_id = document.get("id")
                            
                            print(f"Received PDF with media_id: {media_id}")
                            
                            # 1. Get media URL
                            media_url = get_media_url(media_id)
                            if not media_url:
                                continue
                            
                            # 2. Download media
                            pdf_bytes = download_media(media_url)
                            if not pdf_bytes:
                                continue
                            
                            # 3. Extract text from PDF
                            pdf_text = extract_text_from_pdf_bytes(pdf_bytes)
                            if not pdf_text.strip():
                                print("No text extracted from PDF")
                                continue
                            
                            # 4. Extract info using DeepSeek
                            recipient_info = extract_recipient_info(pdf_text)
                            if not recipient_info:
                                continue
                                
                            name = recipient_info.get("name", "")
                            phone = recipient_info.get("phone", "")
                            
                            if name and phone:
                                print(f"Extracted info - Name: {name}, Phone: {phone}")
                                # 5. Send WhatsApp message to recipient
                                send_whatsapp_message(phone, media_id, name)
                            else:
                                print("Could not extract both name and phone from the PDF.")
    except Exception as e:
        print(f"Error processing webhook: {e}")


@app.get("/webhook")
async def verify_webhook(request: Request):
    """
    Meta API uses this endpoint to verify the webhook setup.
    """
    mode = request.query_params.get("hub.mode")
    token = request.query_params.get("hub.verify_token")
    challenge = request.query_params.get("hub.challenge")

    if mode and token:
        if mode == "subscribe" and token == VERIFY_TOKEN:
            print("Webhook verified successfully!")
            return Response(content=challenge, status_code=200)
        else:
            return Response(content="Forbidden", status_code=403)
            
    return Response(content="Bad Request", status_code=400)


@app.post("/webhook")
async def receive_webhook(request: Request, background_tasks: BackgroundTasks):
    """
    Endpoint to receive incoming WhatsApp messages.
    """
    data = await request.json()
    
    # Process the webhook event in the background to return a 200 OK immediately
    # as required by Meta's webhook documentation.
    background_tasks.add_task(process_webhook_event, data)
    
    return {"status": "ok"}
