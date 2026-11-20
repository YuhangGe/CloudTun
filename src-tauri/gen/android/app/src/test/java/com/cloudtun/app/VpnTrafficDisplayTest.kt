package com.cloudtun.app

import org.junit.Assert.assertEquals
import org.junit.Test

class VpnTrafficDisplayTest {
  @Test fun countsCombinedTrafficUsingActualSampleIntervals() {
    val display = VpnTrafficDisplay()
    assertEquals("速率 0 B/s · 累计 1.0 KB", display.update(1, 1000, 1000))
    assertEquals("速率 2.0 KB/s · 累计 6.0 KB", display.update(1, 3500, 6000))
    assertEquals("速率 0 B/s · 累计 6.0 KB", display.update(1, 5500, 6000))
  }
  @Test fun newSessionAndResetDoNotCreateRateSpikes() {
    val display = VpnTrafficDisplay()
    display.update(1, 10000, 50000)
    assertEquals("速率 0 B/s · 累计 100 B", display.update(2, 1000, 100))
    display.reset()
    assertEquals("速率 0 B/s · 累计 1.0 MB", display.update(2, 3000, 1000000))
  }
  @Test fun repeatedOrRegressingSamplesStayFinite() {
    val display = VpnTrafficDisplay()
    display.update(1, 1000, 1000)
    assertEquals("速率 0 B/s · 累计 1.0 KB", display.update(1, 1000, 1000))
    assertEquals("速率 0 B/s · 累计 1 B", display.update(1, 900, 1))
  }
}
