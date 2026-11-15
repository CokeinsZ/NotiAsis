"""Cupo de mensajería de una corrida (RunQuota) y su cliente del backend."""

import pytest
import requests

from app.models.quota import OFFICE, TRACKING, QuotaReservation
from app.services.backend_client import RustBackendClient
from app.services.quota import RESERVATION_MINUTES, RunQuota


class QuotaBackend:
    """Aprueba hasta `capacity` teléfonos en total; None si `fails`."""

    def __init__(self, capacity=None, fails=False):
        self.capacity = capacity
        self.fails = fails
        self.requests = []
        self.released = []

    def reserve_quota(self, phones, kind, business_id, ttl_minutes):
        self.requests.append((list(phones), kind, business_id, ttl_minutes))
        if self.fails:
            return None
        approved = phones if self.capacity is None else phones[: self.capacity]
        if self.capacity is not None:
            self.capacity -= len(approved)
        return QuotaReservation(tuple(approved), tuple(p for p in phones if p not in approved), len(self.requests))

    def release_quota(self, reservation_id):
        self.released.append(reservation_id)


def test_reserves_the_whole_run_in_one_call_with_normalized_phones():
    backend = QuotaBackend()
    with RunQuota(backend, OFFICE, 2) as quota:
        quota.reserve(["+50251319585", "50251319585", "3105409038", ""])
        assert quota.allows("50251319585")
        assert quota.allows("573105409038")  # 10 dígitos: con indicativo 57, como en el historial

    assert backend.requests == [(["50251319585", "573105409038"], OFFICE, 2, RESERVATION_MINUTES)]
    assert backend.released == [1]


def test_denied_phones_are_not_allowed_and_not_asked_again():
    backend = QuotaBackend(capacity=1)
    with RunQuota(backend, TRACKING, 1) as quota:
        quota.reserve(["573000000001", "573000000002"])
        assert quota.allows("573000000001")
        assert not quota.allows("573000000002")
        assert not quota.allows("573000000002")

    assert len(backend.requests) == 1


def test_phone_outside_the_plan_is_reserved_when_needed():
    # Salió del detalle del pedido: no estaba en la reserva de la corrida.
    backend = QuotaBackend()
    with RunQuota(backend, OFFICE, 1) as quota:
        quota.reserve(["573000000001"])
        assert quota.allows("573000000009")

    assert backend.requests[1][0] == ["573000000009"]
    assert sorted(backend.released) == [1, 2]


def test_without_backend_nothing_is_allowed():
    backend = QuotaBackend(fails=True)
    with RunQuota(backend, OFFICE, 1) as quota:
        quota.reserve(["573000000001"])
        assert quota.unavailable
        assert not quota.allows("573000000001")
    assert backend.released == []


def test_reservations_are_released_even_if_the_run_fails():
    backend = QuotaBackend()
    with pytest.raises(RuntimeError):
        with RunQuota(backend, OFFICE, 1) as quota:
            quota.reserve(["573000000001"])
            raise RuntimeError("boom")
    assert backend.released == [1]


def test_empty_run_does_not_call_the_backend():
    backend = QuotaBackend()
    with RunQuota(backend, OFFICE, 1) as quota:
        quota.reserve([])
    assert backend.requests == []


# ------------------------------ Cliente del backend ------------------------------


class Response:
    def __init__(self, status_code, body=None):
        self.status_code = status_code
        self._body = body or {}
        self.text = str(self._body)

    def raise_for_status(self):
        if self.status_code >= 400:
            raise requests.HTTPError(f"{self.status_code} Error")

    def json(self):
        return self._body


class Session:
    def __init__(self, *responses):
        self.responses = list(responses)
        self.calls = []

    def request(self, method, url, headers=None, timeout=None, **kwargs):
        self.calls.append((method, url, kwargs.get("json")))
        response = self.responses.pop(0)
        if isinstance(response, Exception):
            raise response
        return response


def client_with(*responses):
    client = RustBackendClient("http://backend", api_key="k")
    client._token = "token"
    client._session = Session(*responses)
    return client


APPROVED = {
    "approved": False,
    "approved_phones": ["573000000001"],
    "denied_phones": ["573000000002"],
    "reservation_id": 12,
}


def test_client_reserves_and_releases_quota():
    client = client_with(Response(200, APPROVED), Response(200, {"released": True}))

    reservation = client.reserve_quota(["573000000001", "573000000002"], TRACKING, 1, 60)
    client.release_quota(12)

    assert reservation == QuotaReservation(("573000000001",), ("573000000002",), 12)
    assert client._session.calls == [
        ("POST", "http://backend/quota/reservations",
         {"phones": ["573000000001", "573000000002"], "kind": "tracking", "business_id": 1, "ttl_minutes": 60}),
        ("DELETE", "http://backend/quota/reservations/12", None),
    ]


def test_client_retries_while_the_quota_is_busy(monkeypatch):
    monkeypatch.setattr("app.services.backend_client.time.sleep", lambda _: None)
    client = client_with(Response(503), Response(503), Response(200, APPROVED))

    assert client.reserve_quota(["573000000001"], OFFICE, 1, 60).reservation_id == 12
    assert len(client._session.calls) == 3


@pytest.mark.parametrize("response", [Response(503), Response(500), Response(400), requests.ConnectionError("down")])
def test_client_returns_none_when_quota_cannot_be_reserved(monkeypatch, response):
    monkeypatch.setattr("app.services.backend_client.time.sleep", lambda _: None)
    client = client_with(response, response, response)
    assert client.reserve_quota(["573000000001"], OFFICE, 1, 60) is None
