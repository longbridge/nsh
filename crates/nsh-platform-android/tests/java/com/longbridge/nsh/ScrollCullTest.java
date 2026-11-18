package com.longbridge.nsh;

/**
 * Coverage for which children a scroll view's content draws: the margin,
 * what a row may draw in (its overflow, shadows, outline, elevation and
 * shader), rows outside the viewport and the margin, the transforms that
 * keep a row drawn, a descendant's transform that pins its row and when a
 * scroll draws the rows again. Plain JDK, no Android APIs:
 *
 * <pre>
 * javac -d target/java-tests \
 *   crates/nsh-platform-android/java/com/longbridge/nsh/ScrollCull.java \
 *   crates/nsh-platform-android/tests/java/com/longbridge/nsh/ScrollCullTest.java
 * java -cp target/java-tests com.longbridge.nsh.ScrollCullTest
 * </pre>
 */
public final class ScrollCullTest {
    public static void main(String[] args) {
        // A quarter of the larger side, at least 64 dp.
        check("a phone held upright: a quarter of its height", ScrollCull.margin(1080, 2000, 3) == 500);
        check("a small viewport: 64 dp", ScrollCull.margin(600, 300, 3) == 192);
        check("a wide horizontal list: a quarter of its width", ScrollCull.margin(2000, 200, 1) == 500);
        check("the least margin follows the density", ScrollCull.margin(100, 100, 2.625f) == 168);

        // A vertical viewport 0..1000 down to 5000..6000, margin 250.
        int m = 250;
        check("a row in the viewport is drawn", !ScrollCull.outside(0, 5100, 1080, 5200, 0, 5000, 1080, 6000, m));
        check("a row across the top edge is drawn", !ScrollCull.outside(0, 4950, 1080, 5050, 0, 5000, 1080, 6000, m));
        check("a row within the margin above is drawn", !ScrollCull.outside(0, 4700, 1080, 4800, 0, 5000, 1080, 6000, m));
        check("a row within the margin below is drawn", !ScrollCull.outside(0, 6200, 1080, 6300, 0, 5000, 1080, 6000, m));
        check("a row touching the margin is drawn", !ScrollCull.outside(0, 4650, 1080, 4750, 0, 5000, 1080, 6000, m));
        check("a row past the margin above is left out", ScrollCull.outside(0, 4600, 1080, 4700, 0, 5000, 1080, 6000, m));
        check("a row past the margin below is left out", ScrollCull.outside(0, 6300, 1080, 6400, 0, 5000, 1080, 6000, m));
        check("a tall row spanning the viewport is drawn", !ScrollCull.outside(0, 0, 1080, 9000, 0, 5000, 1080, 6000, m));

        // A horizontal viewport 3000..4000 across, 300 high.
        check("a cell in a horizontal viewport is drawn", !ScrollCull.outside(3200, 0, 3400, 300, 3000, 0, 4000, 300, m));
        check("a cell left of it past the margin is left out", ScrollCull.outside(2500, 0, 2700, 300, 3000, 0, 4000, 300, m));
        check("a cell right of it past the margin is left out", ScrollCull.outside(4300, 0, 4500, 300, 3000, 0, 4000, 300, m));
        check("a cell right of it within the margin is drawn", !ScrollCull.outside(4200, 0, 4400, 300, 3000, 0, 4000, 300, m));
        // A child placed beside the content's cross axis (a row's dropdown
        // under a horizontal list) is outside along that axis too.
        check("a cell far below a horizontal viewport is left out", ScrollCull.outside(3200, 900, 3400, 1100, 3000, 0, 4000, 300, m));

        // Only a child standing where its bounds say may be left out.
        check("an untransformed child may be left out", ScrollCull.identity(0, 0, 1, 1, 0, 0, 0));
        check("a translated child is drawn", !ScrollCull.identity(0, -400, 1, 1, 0, 0, 0));
        check("a child translated across is drawn", !ScrollCull.identity(12, 0, 1, 1, 0, 0, 0));
        check("a scaled child is drawn", !ScrollCull.identity(0, 0, 1.5f, 1.5f, 0, 0, 0));
        check("a mirrored cell is drawn", !ScrollCull.identity(0, 0, -1, 1, 0, 0, 0));
        check("a child growing in from nothing is drawn", !ScrollCull.identity(0, 0, 0, 0, 0, 0, 0));
        check("a rotated child is drawn", !ScrollCull.identity(0, 0, 1, 1, 90, 0, 0));
        check("a child flipped in 3D is drawn", !ScrollCull.identity(0, 0, 1, 1, 0, 0, 30));
        check("a child tilted in 3D is drawn", !ScrollCull.identity(0, 0, 1, 1, 0, 15, 0));

        // What a row may draw in: bounds, its descendants' overflow, its decoration.
        int[] r = new int[4];
        ScrollCull.reach(0, 1300, 1000, 1400, null, null, r);
        check("a plain row reaches its bounds", same(r, 0, 1300, 1000, 1400));
        ScrollCull.reach(0, 1300, 1000, 1400, new float[] {-10.5f, -300.25f, 1000.5f, 100}, null, r);
        check("overflow above and left widens it, rounded outwards", same(r, -11, 999, 1001, 1400));
        ScrollCull.reach(0, 1300, 1000, 1400, new float[] {10, 20, 900, 400.1f}, null, r);
        check("overflow inside never narrows it, below widens it", same(r, 0, 1300, 1000, 1701));
        ScrollCull.reach(0, 1300, 1000, 1400, null, new float[] {2.5f, 500, 0, 0.1f}, r);
        check("paint extents widen it outwards", same(r, -3, 800, 1000, 1401));
        ScrollCull.reach(0, 1300, 1000, 1400, new float[] {0, -100, 1000, 100}, new float[] {0, 50, 0, 0}, r);
        check("overflow and paint take the farther each side", same(r, 0, 1200, 1000, 1400));

        // A shadow 500 px above a row 1300..1400 below a 0..1000 viewport (margin 250) shows at 800..900.
        float[] p = new float[4];
        check("a row with no decoration paints nothing outside",
                !ScrollCull.paint(new float[0], new int[0], 0, 0, 0, 0, 0, p));
        check("a shadow moved up paints above",
                ScrollCull.paint(new float[] {0, -500, 0, 0, 0}, new int[] {0x73000000}, 0, 0, 0, 0, 0, p)
                        && p[1] == 500 && p[3] == 0 && p[0] == 0 && p[2] == 0);
        ScrollCull.reach(0, 1300, 1000, 1400, null, p, r);
        check("so the row is drawn", !ScrollCull.outside(r, 0, 0, 1000, 1000, m));
        check("without it the row is left out", ScrollCull.outside(0, 1300, 1000, 1400, 0, 0, 1000, 1000, m));
        ScrollCull.paint(new float[] {4, 8, 18, 2, 0}, new int[] {0x73000000}, 0, 0, 0, 0, 0, p);
        check("a blurred, spread shadow reaches 1.5 blurs past its spread, offset",
                p[0] == 25 && p[1] == 21 && p[2] == 33 && p[3] == 37);
        check("an inset shadow paints inside",
                !ScrollCull.paint(new float[] {0, -500, 40, 0, 1}, new int[] {0xff000000}, 0, 0, 0, 0, 0, p));
        check("a transparent shadow paints nothing",
                !ScrollCull.paint(new float[] {0, -500, 40, 0, 0}, new int[] {0x00ffffff}, 0, 0, 0, 0, 0, p));
        check("a negative spread shrinks a shadow, never below nothing",
                !ScrollCull.paint(new float[] {0, 0, 4, -20, 0}, new int[] {0xff000000}, 0, 0, 0, 0, 0, p));
        check("an outline paints its offset and width all round",
                ScrollCull.paint(null, null, 3, 0xff3366ff, 4, 0, 0, p) && p[0] == 7 && p[1] == 7 && p[2] == 7 && p[3] == 7);
        check("a transparent outline paints nothing", !ScrollCull.paint(null, null, 3, 0x003366ff, 4, 0, 0, p));
        check("an outline inset past its width paints nothing", !ScrollCull.paint(null, null, 2, 0xff000000, -5, 0, 0, p));
        check("elevation paints three times its z all round",
                ScrollCull.paint(null, null, 0, 0, 0, 21, 0, p) && p[0] == 63 && p[3] == 63);
        check("a shader's overflow paints all round", ScrollCull.paint(null, null, 0, 0, 0, 0, 90, p) && p[2] == 90);
        ScrollCull.paint(new float[] {0, 30, 0, 0, 0}, new int[] {0xff000000}, 0, 0, 0, 5, 0, p);
        check("layers and kinds take the farthest each side", p[1] == 15 && p[3] == 30);

        // A descendant's transform: how far past its own 100 x 100 px bounds it goes.
        check("in place it goes nowhere", ScrollCull.excess(0, 0, 100, 100, 100, 100) == 0);
        check("shrunk it stays inside", ScrollCull.excess(25, 25, 75, 75, 100, 100) == 0);
        check("a quarter turn of a square stays inside", ScrollCull.excess(-0.00001f, 0, 100.00001f, 100, 100, 100) < 0.001f);
        check("translated up it goes that far", ScrollCull.excess(0, -500, 100, -400, 100, 100) == 500);
        check("translated right it goes that far", ScrollCull.excess(30, 0, 130, 100, 100, 100) == 30);
        check("doubled about its center it goes half its side",
                ScrollCull.excess(-50, -50, 150, 150, 100, 100) == 50);
        check("an unmappable transform cannot be told", Float.isNaN(ScrollCull.excess(Float.NaN, 0, 1, 1, 1, 1)));
        int limit = 42; // 16 dp at 2.625
        check("a small move does not pin its row", !ScrollCull.pins(30, limit, false));
        check("a move to the limit does not pin its row", !ScrollCull.pins(42, limit, false));
        check("a move past it pins its row", ScrollCull.pins(500, limit, false));
        check("what cannot be told pins its row", ScrollCull.pins(Float.NaN, limit, false));
        check("under a clipping ancestor nothing pins", !ScrollCull.pins(500, limit, true));

        // Drawn for the viewport 0..1000 down to 5000..6000, margin 250: a
        // scroll draws again once an edge goes more than 125 past.
        int[] drawn = {0, 5000, 1080, 6000};
        check("a still viewport needs nothing", !ScrollCull.stale(drawn, m, 0, 5000, 1080, 6000));
        check("scrolling down by half the margin needs nothing", !ScrollCull.stale(drawn, m, 0, 5125, 1080, 6125));
        check("scrolling down past half the margin draws again", ScrollCull.stale(drawn, m, 0, 5126, 1080, 6126));
        check("scrolling up past half the margin draws again", ScrollCull.stale(drawn, m, 0, 4874, 1080, 5874));
        check("a jump far away draws again", ScrollCull.stale(drawn, m, 0, 90000, 1080, 91000));
        check("scrolling across past half the margin draws again", ScrollCull.stale(drawn, m, 126, 5000, 1206, 6000));
        check("a viewport grown past half the margin draws again", ScrollCull.stale(drawn, m, 0, 5000, 1080, 6400));
        check("a viewport that shrinks needs nothing", !ScrollCull.stale(drawn, m, 0, 5000, 1080, 5500));

        // What a scroll within half the margin shows was drawn: every child
        // meeting the new viewport is within the margin of the drawn one.
        int[] now = {0, 5125, 1080, 6125};
        check("rows coming into view were drawn",
                !ScrollCull.stale(drawn, m, now[0], now[1], now[2], now[3])
                        && !ScrollCull.outside(0, 6100, 1080, 6200, drawn[0], drawn[1], drawn[2], drawn[3], m));
        System.out.println("ScrollCullTest: ok");
    }

    private static boolean same(int[] r, int l, int t, int ri, int b) {
        return r[0] == l && r[1] == t && r[2] == ri && r[3] == b;
    }

    private static void check(String what, boolean ok) {
        if (!ok) throw new AssertionError(what);
    }
}
