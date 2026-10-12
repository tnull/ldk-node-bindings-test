package org.lightningdevkit.ldknode

import kotlin.test.Test
import kotlin.test.assertEquals

class NativeSmokeTest {
    @Test
    fun defaultConfigLoadsNativeLibrary() {
        assertEquals(Network.BITCOIN, defaultConfig().network)
    }
}
