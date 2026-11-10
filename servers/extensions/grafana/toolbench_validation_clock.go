// This file is added to github.com/grafana/mcp-grafana/v2@v2.1.1/tools by
// tool-bench (servers/extensions/grafana/build.sh). It is not upstream code.

package tools

import (
	"errors"
	"os"
	"time"
)

// silenceValidationNowEnv names the declared business instant used by the
// alerting_silences_write "window already closed" check.
const silenceValidationNowEnv = "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW"

// errSilenceValidationClock is deliberately neutral: it reaches the tool
// caller, so it names neither the benchmark nor the variable.
var errSilenceValidationClock = errors.New("silence validation clock is misconfigured: expected an RFC3339 instant")

// silenceValidationNow returns the instant that validateWritePayload compares
// ends_at against.
//
// Unset: the upstream behavior, time.Now(). Set: exactly that RFC3339
// instant, or an error (an empty or malformed value never falls back to the
// process clock). It is called from that one business-payload check only. It
// does not replace time.Now, and TLS verification, authentication, HTTP
// timeouts, logging and every other mcp-grafana time default keep reading the
// real process clock.
func silenceValidationNow() (time.Time, error) {
	value, ok := os.LookupEnv(silenceValidationNowEnv)
	if !ok {
		return time.Now(), nil
	}
	instant, err := time.Parse(time.RFC3339, value)
	if err != nil {
		return time.Time{}, errSilenceValidationClock
	}
	return instant, nil
}
