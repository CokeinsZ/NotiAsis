#!/bin/bash

PHONE_NUMBERS=(
    "573003579384"
    "573205363052"
    "573126866924"
    "573116143785"
)

JWT_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzdGl2ZW4uYWRtaW4iLCJraW5kIjoiYXNzb2NpYXRlIiwiYnVzaW5lc3NfaWQiOjEsInBob25lX251bWJlciI6IjU3MzAwMzU3OTM4NCIsImlhdCI6MTc4NzQxMjM2MywiZXhwIjoxNzg3NDEzMjYzfQ._u2x4piilqX5YfS4LactNZzl0syELz23A1bCb53en9s"

for index in "${!PHONE_NUMBERS[@]}"; do
    curl -X POST http://localhost:32768/businesses/1/associates \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $JWT_TOKEN" \
      -d "{
        \"phone_number\": \"${PHONE_NUMBERS[$index]}\",
        \"username\": \"victor.$((index + 1))\",
        \"password\": \"Passw0rd\"
      }"
    echo
done