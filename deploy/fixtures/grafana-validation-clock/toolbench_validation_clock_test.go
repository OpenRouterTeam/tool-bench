// Behavioral tests for the tool-bench silence validation-clock adaptation.
//
// deploy/test-grafana-validation-clock.sh copies this file into the patched
// github.com/grafana/mcp-grafana/v2@v2.1.1/tools package, so every call below
// runs the pinned upstream validation, alerting client, transport, TLS and
// authentication code. All identifiers are prefixed tb to avoid upstream test
// helpers.

package tools

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/json"
	"encoding/pem"
	"errors"
	"io"
	"math/big"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	mcpgrafana "github.com/grafana/mcp-grafana/v2"
)

// The authored ops/grafana/silence-replica-maintenance business clock and window.
const (
	tbScenarioNow = "2026-11-12T15:30:00Z"
	tbWindowStart = "2026-11-13T02:00:00Z"
	tbWindowEnd   = "2026-11-13T04:00:00Z"
	tbToken       = "glsa_validation_clock_fixture"
)

func tbStr(s string) *string { return &s }

func tbSetClock(t *testing.T, value string) {
	t.Helper()
	t.Setenv(silenceValidationNowEnv, value)
}

func tbUnsetClock(t *testing.T) {
	t.Helper()
	t.Setenv(silenceValidationNowEnv, "") // restores the prior value after the test
	if err := os.Unsetenv(silenceValidationNowEnv); err != nil {
		t.Fatal(err)
	}
}

func tbCreate(start, end string) ManageSilencesParams {
	return ManageSilencesParams{
		Operation: "create",
		Matchers: []SilenceMatcherParam{
			{Name: "alertname", Value: "PostgresReplicationLag"},
			{Name: "instance", Value: "pg-replica-2"},
		},
		StartsAt:  tbStr(start),
		EndsAt:    tbStr(end),
		Comment:   tbStr("CHG-7781 storage maintenance on pg-replica-2"),
		CreatedBy: tbStr("sre"),
	}
}

func tbUpdate(start, end string) ManageSilencesParams {
	p := tbCreate(start, end)
	p.Operation = "update"
	p.SilenceID = tbStr("8d2e4f6a-1b3c-4d5e-9f0a-7b8c9d0e1f2a")
	return p
}

func tbMustContain(t *testing.T, err error, want string) {
	t.Helper()
	if err == nil {
		t.Fatalf("expected error containing %q, got nil", want)
	}
	if !strings.Contains(err.Error(), want) {
		t.Fatalf("expected error containing %q, got %q", want, err)
	}
}

func TestToolBenchSeamUnsetIsTheProcessClock(t *testing.T) {
	tbUnsetClock(t)
	before := time.Now()
	got, err := silenceValidationNow()
	after := time.Now()
	if err != nil {
		t.Fatal(err)
	}
	if got.Before(before) || got.After(after) {
		t.Fatalf("unset seam returned %s outside the real-clock interval [%s, %s]", got, before, after)
	}
}

func TestToolBenchSeamReturnsTheDeclaredInstantExactly(t *testing.T) {
	want, _ := time.Parse(time.RFC3339, tbScenarioNow)
	for _, value := range []string{tbScenarioNow, "2026-11-12T10:30:00-05:00", "2026-11-12T15:30:00.000Z"} {
		tbSetClock(t, value)
		got, err := silenceValidationNow()
		if err != nil {
			t.Fatalf("%s: %v", value, err)
		}
		if !got.Equal(want) {
			t.Fatalf("%s: got %s, want %s", value, got, want)
		}
	}
}

func TestToolBenchSeamFailsClosedOnMalformedValues(t *testing.T) {
	for _, value := range []string{"", " ", "2026-11-12", "2026-11-12T15:30:00", "2026-11-12 15:30:00Z", "now", "1794497400"} {
		tbSetClock(t, value)
		got, err := silenceValidationNow()
		if !errors.Is(err, errSilenceValidationClock) {
			t.Fatalf("%q: expected misconfiguration error, got %s / %v", value, got, err)
		}
		if !got.IsZero() {
			t.Fatalf("%q: a malformed value must not yield a usable instant, got %s", value, got)
		}
		msg := strings.ToLower(err.Error())
		if strings.Contains(msg, "bench") || strings.Contains(msg, "tool_bench") {
			t.Fatalf("model-visible error names the benchmark: %q", err)
		}
	}
}

// The declared instant does not move the process clock (no global override).
func TestToolBenchDeclaredInstantDoesNotChangeTimeNow(t *testing.T) {
	tbSetClock(t, "2001-01-01T00:00:00Z")
	before := time.Now()
	if _, err := silenceValidationNow(); err != nil {
		t.Fatal(err)
	}
	now := time.Now()
	if now.Before(before) || now.Year() < 2025 {
		t.Fatalf("time.Now moved with the declared validation instant: %s", now)
	}
}

func TestToolBenchAuthoredWindowAgainstDeclaredInstant(t *testing.T) {
	cases := []struct {
		name, clock string
		params      ManageSilencesParams
		wantErr     string
	}{
		{"create at scenario now", tbScenarioNow, tbCreate(tbWindowStart, tbWindowEnd), ""},
		{"update at scenario now", tbScenarioNow, tbUpdate(tbWindowStart, tbWindowEnd), ""},
		{"one second before end", "2026-11-13T03:59:59Z", tbCreate(tbWindowStart, tbWindowEnd), ""},
		{"exactly at end is closed", tbWindowEnd, tbCreate(tbWindowStart, tbWindowEnd), "is in the past, so the silence would be expired on creation"},
		{"after end create", "2026-11-13T04:00:01Z", tbCreate(tbWindowStart, tbWindowEnd), "is in the past, so the silence would be expired on creation"},
		{"after end update", "2026-11-13T04:00:01Z", tbUpdate(tbWindowStart, tbWindowEnd), "use the 'delete' operation to expire a silence early"},
		{"offset clock after end", "2026-11-12T23:00:01-05:00", tbCreate(tbWindowStart, tbWindowEnd), "is in the past"},
		// A declared instant far in the past accepts a window that closed long
		// before the real clock: the process clock is not consulted when set.
		{"declared past instant", "2000-01-01T00:00:00Z", tbCreate("2000-01-01T01:00:00Z", "2000-01-01T02:00:00Z"), ""},
		{"declared past instant expired", "2000-01-01T03:00:00Z", tbCreate("2000-01-01T01:00:00Z", "2000-01-01T02:00:00Z"), "is in the past"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			tbSetClock(t, tc.clock)
			err := tc.params.validate()
			if tc.wantErr == "" {
				if err != nil {
					t.Fatalf("unexpected error: %v", err)
				}
				return
			}
			tbMustContain(t, err, tc.wantErr)
		})
	}
}

// Unset keeps upstream real-clock behavior on both sides of expiry.
func TestToolBenchUnsetPreservesUpstreamRealClockCheck(t *testing.T) {
	tbUnsetClock(t)
	wall := time.Now().UTC()
	open := tbCreate(wall.Add(time.Hour).Format(time.RFC3339), wall.Add(2*time.Hour).Format(time.RFC3339))
	if err := open.validate(); err != nil {
		t.Fatalf("open real-clock window rejected: %v", err)
	}
	closed := tbCreate(wall.Add(-2*time.Hour).Format(time.RFC3339), wall.Add(-time.Hour).Format(time.RFC3339))
	tbMustContain(t, closed.validate(), "is in the past")
	authored := tbCreate(tbWindowStart, tbWindowEnd)
	end, _ := time.Parse(time.RFC3339, tbWindowEnd)
	if err := authored.validate(); end.After(time.Now()) {
		if err != nil {
			t.Fatalf("authored window rejected before real expiry: %v", err)
		}
	} else {
		tbMustContain(t, err, "is in the past")
	}
}

func TestToolBenchInvalidPayloadsStillRejected(t *testing.T) {
	tbSetClock(t, tbScenarioNow)
	mutate := func(f func(*ManageSilencesParams)) ManageSilencesParams {
		p := tbCreate(tbWindowStart, tbWindowEnd)
		f(&p)
		return p
	}
	cases := []struct {
		name    string
		params  ManageSilencesParams
		wantErr string
	}{
		{"no matchers", mutate(func(p *ManageSilencesParams) { p.Matchers = nil }), "matchers is required"},
		{"empty matcher name", mutate(func(p *ManageSilencesParams) { p.Matchers[0].Name = "" }), "matcher at index 0: name is required"},
		{"missing start", mutate(func(p *ManageSilencesParams) { p.StartsAt = nil }), "starts_at is required"},
		{"missing end", mutate(func(p *ManageSilencesParams) { p.EndsAt = nil }), "ends_at is required"},
		{"non-RFC3339 end", mutate(func(p *ManageSilencesParams) { p.EndsAt = tbStr("2026-11-13 04:00") }), "ends_at must be a valid RFC3339 timestamp"},
		{"end before start", mutate(func(p *ManageSilencesParams) { p.EndsAt = tbStr("2026-11-13T01:00:00Z") }), "must be after starts_at"},
		{"end equals start", mutate(func(p *ManageSilencesParams) { p.EndsAt = tbStr(tbWindowStart) }), "must be after starts_at"},
		{"missing comment", mutate(func(p *ManageSilencesParams) { p.Comment = nil }), "comment is required"},
		{"empty comment", mutate(func(p *ManageSilencesParams) { p.Comment = tbStr("") }), "comment is required"},
		{"rule uid on write", mutate(func(p *ManageSilencesParams) { p.RuleUID = tbStr("pg-replication-lag") }), "rule_uid must not be set"},
		{"silence id on create", mutate(func(p *ManageSilencesParams) { p.SilenceID = tbStr("x") }), "silence_id must not be set for 'create'"},
		{"update without id", mutate(func(p *ManageSilencesParams) { p.Operation = "update" }), "silence_id is required for 'update'"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) { tbMustContain(t, tc.params.validate(), tc.wantErr) })
	}
}

// Earlier upstream checks keep their order; a misconfigured clock never turns
// an invalid or valid window into success.
func TestToolBenchMisconfiguredClockOrderingAndNoSuccess(t *testing.T) {
	tbSetClock(t, "not-a-time")
	noMatchers := tbCreate(tbWindowStart, tbWindowEnd)
	noMatchers.Matchers = nil
	tbMustContain(t, noMatchers.validate(), "matchers is required")
	reversed := tbCreate(tbWindowEnd, tbWindowStart)
	tbMustContain(t, reversed.validate(), "must be after starts_at")
	valid := tbCreate(tbWindowStart, tbWindowEnd)
	if err := valid.validate(); !errors.Is(err, errSilenceValidationClock) {
		t.Fatalf("expected misconfiguration error, got %v", err)
	}
}

// ── HTTP/TLS/auth: the real pinned alerting client over a TLS fixture ──────

type tbRequest struct {
	Method, Path, Authorization, UserAgent string
	Body                                   map[string]any
}

type tbGrafana struct {
	server *httptest.Server
	caFile string
	mu     sync.Mutex
	seen   []tbRequest
}

func (g *tbGrafana) requests() []tbRequest {
	g.mu.Lock()
	defer g.mu.Unlock()
	return append([]tbRequest(nil), g.seen...)
}

// tbStartGrafana serves a minimal Alertmanager silences API over TLS with a
// fixture CA and a leaf valid only within [notBefore, notAfter].
func tbStartGrafana(t *testing.T, notBefore, notAfter time.Time) *tbGrafana {
	t.Helper()
	caKey, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	caTemplate := &x509.Certificate{
		SerialNumber:          big.NewInt(1),
		Subject:               pkix.Name{CommonName: "validation-clock fixture CA"},
		NotBefore:             time.Date(1990, 1, 1, 0, 0, 0, 0, time.UTC),
		NotAfter:              time.Date(2200, 1, 1, 0, 0, 0, 0, time.UTC),
		IsCA:                  true,
		BasicConstraintsValid: true,
		KeyUsage:              x509.KeyUsageCertSign,
	}
	caDER, err := x509.CreateCertificate(rand.Reader, caTemplate, caTemplate, &caKey.PublicKey, caKey)
	if err != nil {
		t.Fatal(err)
	}
	ca, _ := x509.ParseCertificate(caDER)
	leafKey, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	leafTemplate := &x509.Certificate{
		SerialNumber: big.NewInt(2),
		Subject:      pkix.Name{CommonName: "127.0.0.1"},
		NotBefore:    notBefore,
		NotAfter:     notAfter,
		IPAddresses:  []net.IP{net.ParseIP("127.0.0.1")},
		KeyUsage:     x509.KeyUsageDigitalSignature,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
	}
	leafDER, err := x509.CreateCertificate(rand.Reader, leafTemplate, ca, &leafKey.PublicKey, caKey)
	if err != nil {
		t.Fatal(err)
	}
	caFile := filepath.Join(t.TempDir(), "ca.pem")
	if err := os.WriteFile(caFile, pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: caDER}), 0o600); err != nil {
		t.Fatal(err)
	}

	g := &tbGrafana{caFile: caFile}
	g.server = httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		req := tbRequest{Method: r.Method, Path: r.URL.Path, Authorization: r.Header.Get("Authorization"), UserAgent: r.Header.Get("User-Agent")}
		if len(raw) > 0 {
			_ = json.Unmarshal(raw, &req.Body)
		}
		g.mu.Lock()
		g.seen = append(g.seen, req)
		g.mu.Unlock()
		w.Header().Set("Content-Type", "application/json")
		switch {
		case r.Method == http.MethodPost && r.URL.Path == silencesBasePath+"/silences":
			_, _ = w.Write([]byte(`{"silenceID":"fixture-silence-1"}`))
		case r.Method == http.MethodGet && r.URL.Path == silencesBasePath+"/silences":
			_, _ = w.Write([]byte(`[]`))
		case r.Method == http.MethodDelete:
			w.WriteHeader(http.StatusOK)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	g.server.TLS = &tls.Config{Certificates: []tls.Certificate{{Certificate: [][]byte{leafDER}, PrivateKey: leafKey}}}
	g.server.StartTLS()
	t.Cleanup(g.server.Close)
	return g
}

func (g *tbGrafana) ctx() context.Context {
	return mcpgrafana.WithGrafanaConfig(context.Background(), mcpgrafana.GrafanaConfig{
		URL:       g.server.URL,
		APIKey:    tbToken,
		TLSConfig: &mcpgrafana.TLSConfig{CAFile: g.caFile},
	})
}

// tbStartGrafanaNow uses a leaf valid only within one hour of the real clock.
func tbStartGrafanaNow(t *testing.T) *tbGrafana {
	now := time.Now()
	return tbStartGrafana(t, now.Add(-time.Hour), now.Add(time.Hour))
}

func TestToolBenchHandlerPostsAuthoredWindowAtScenarioNow(t *testing.T) {
	g := tbStartGrafanaNow(t)
	tbSetClock(t, tbScenarioNow)
	out, err := manageSilencesReadWrite(g.ctx(), tbCreate(tbWindowStart, tbWindowEnd))
	if err != nil {
		t.Fatalf("create failed: %v", err)
	}
	if resp, ok := out.(*createSilenceResponse); !ok || resp.SilenceID != "fixture-silence-1" {
		t.Fatalf("unexpected result %#v", out)
	}
	reqs := g.requests()
	if len(reqs) != 1 || reqs[0].Method != http.MethodPost || reqs[0].Path != silencesBasePath+"/silences" {
		t.Fatalf("expected exactly one silences POST, got %#v", reqs)
	}
	if reqs[0].Authorization != "Bearer "+tbToken {
		t.Fatalf("authorization changed: %q", reqs[0].Authorization)
	}
	if !strings.HasPrefix(reqs[0].UserAgent, "mcp-grafana/") {
		t.Fatalf("user agent changed: %q", reqs[0].UserAgent)
	}
	for field, want := range map[string]string{"startsAt": tbWindowStart, "endsAt": tbWindowEnd} {
		raw, _ := reqs[0].Body[field].(string)
		got, err := time.Parse(time.RFC3339, raw)
		wantTime, _ := time.Parse(time.RFC3339, want)
		if err != nil || !got.Equal(wantTime) {
			t.Fatalf("%s on the wire = %q, want instant %s", field, raw, want)
		}
	}
	if reqs[0].Body["comment"] != "CHG-7781 storage maintenance on pg-replica-2" || reqs[0].Body["createdBy"] != "sre" {
		t.Fatalf("payload changed: %#v", reqs[0].Body)
	}
}

func TestToolBenchHandlerRejectsBeforeHTTP(t *testing.T) {
	for _, tc := range []struct{ name, clock, want string }{
		{"expired at declared instant", "2026-11-13T04:00:01Z", "is in the past"},
		{"misconfigured clock", "garbage", errSilenceValidationClock.Error()},
		{"empty clock", "", errSilenceValidationClock.Error()},
	} {
		t.Run(tc.name, func(t *testing.T) {
			g := tbStartGrafanaNow(t)
			tbSetClock(t, tc.clock)
			out, err := manageSilencesReadWrite(g.ctx(), tbCreate(tbWindowStart, tbWindowEnd))
			tbMustContain(t, err, tc.want)
			if out != nil {
				t.Fatalf("rejected call returned a result: %#v", out)
			}
			if reqs := g.requests(); len(reqs) != 0 {
				t.Fatalf("rejected call reached HTTP: %#v", reqs)
			}
		})
	}
}

// Read and delete paths never consult the seam, even when it is misconfigured.
func TestToolBenchSeamIsLimitedToWritePayloadValidation(t *testing.T) {
	g := tbStartGrafanaNow(t)
	tbSetClock(t, "garbage")
	if _, err := manageSilencesRead(g.ctx(), ManageSilencesReadParams{Operation: "list"}); err != nil {
		t.Fatalf("list consulted the validation clock: %v", err)
	}
	if _, err := manageSilencesReadWrite(g.ctx(), ManageSilencesParams{Operation: "delete", SilenceID: tbStr("fixture-silence-1")}); err != nil {
		t.Fatalf("delete consulted the validation clock: %v", err)
	}
	if reqs := g.requests(); len(reqs) != 2 {
		t.Fatalf("expected list and delete requests, got %#v", reqs)
	}
}

// TLS verification uses the real process clock: a certificate valid only now
// is accepted while the declared business instant is in 2100.
func TestToolBenchTLSIgnoresDeclaredInstantFutureBusinessTime(t *testing.T) {
	g := tbStartGrafanaNow(t)
	tbSetClock(t, "2100-01-01T00:00:00Z")
	if _, err := manageSilencesReadWrite(g.ctx(), tbCreate("2100-01-01T02:00:00Z", "2100-01-01T04:00:00Z")); err != nil {
		t.Fatalf("TLS/auth followed the declared instant: %v", err)
	}
	reqs := g.requests()
	if len(reqs) != 1 || reqs[0].Authorization != "Bearer "+tbToken {
		t.Fatalf("unexpected requests %#v", reqs)
	}
}

// Conversely, a certificate valid only around the declared instant (and not
// now) is rejected: setting the business instant cannot make TLS accept it.
func TestToolBenchTLSRejectsCertificateValidOnlyAtDeclaredInstant(t *testing.T) {
	g := tbStartGrafana(t, time.Date(2100, 1, 1, 0, 0, 0, 0, time.UTC), time.Date(2101, 1, 1, 0, 0, 0, 0, time.UTC))
	tbSetClock(t, "2100-06-01T00:00:00Z")
	params := tbCreate("2100-06-01T02:00:00Z", "2100-06-01T04:00:00Z")
	if err := params.validate(); err != nil {
		t.Fatalf("business validation should pass at the declared instant: %v", err)
	}
	_, err := manageSilencesReadWrite(g.ctx(), params)
	var invalid x509.CertificateInvalidError
	if !errors.As(err, &invalid) || invalid.Reason != x509.Expired {
		t.Fatalf("expected an x509 validity-period failure from the real clock, got %v", err)
	}
	if reqs := g.requests(); len(reqs) != 0 {
		t.Fatalf("request passed TLS with a certificate not valid now: %#v", reqs)
	}
}
