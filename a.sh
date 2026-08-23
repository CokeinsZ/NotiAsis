#!/bin/bash

GRAPH_API_URL="https://graph.facebook.com/v19.0" 
WHATSAPP_PHONE_ID="1272181372636506"
WHATSAPP_TOKEN="EAAiGjJJtDQABR69u5PiLHP8BQyi58utJ0VOE0h3oQZBrwZBLJVtKAnbTZAnqxX5BFYr1GQM1XXowriYCsaDAr4URZBxgvXmEyp12PMsa9ll3HCy3JA2NgQ5aZB0B7nCvaEmnqR8x7IbH5WbwH2WkJynhgHWWfu9WSEOtjGrdR6apFRJUz0oddOdl66bRebsnArAZDZD"

# Número del destinatario con código de país (Ej: 573001234567 para Colombia)
TO_NUMBER="573108353605" 

# El mensaje de texto libre que deseas enviar
MESSAGE_BODY="Cuentamelo todo"

# ==========================================
# COMANDO CURL
# ==========================================
echo "Enviando mensaje a $TO_NUMBER..."

curl -X POST "${GRAPH_API_URL}/${WHATSAPP_PHONE_ID}/messages" \
  -H "Authorization: Bearer ${WHATSAPP_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "messaging_product": "whatsapp",
    "recipient_type": "individual",
    "to": "'"${TO_NUMBER}"'",
    "type": "text",
    "text": {
      "preview_url": false,
      "body": "'"${MESSAGE_BODY}"'"
    }
  }'

echo -e "\nEjecución finalizada."