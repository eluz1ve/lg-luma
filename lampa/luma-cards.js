/*
 * A Lampa plugin that opens the card Luma asks for. On webOS, Lampa reads a card only from
 * "?card=" in its own address, while Luma's launch parameters ({card, media, source}) arrive in
 * PalmSystem.launchParams; this reads them at start and whenever Luma relaunches Lampa.
 * Old webOS browsers run it, so it stays ES5.
 */
(function () {
    'use strict';

    function params() {
        try {
            return JSON.parse((window.PalmSystem && window.PalmSystem.launchParams) || '{}') || {};
        } catch (e) {
            return {};
        }
    }

    function link() {
        var p = params();
        var id = String(p.card || '');
        if (!/^[0-9]+$/.test(id)) return null;
        var source = p.source || 'cub';
        return {
            id: id,
            component: 'full',
            method: p.media === 'tv' ? 'tv' : 'movie',
            source: source,
            card: { id: id, source: source }
        };
    }

    function open() {
        var card = link();
        if (card) Lampa.Activity.push(card);
    }

    if (window.luma_cards) return;
    window.luma_cards = true;

    if (window.appready) open();
    else {
        Lampa.Listener.follow('app', function (e) {
            if (e.type === 'ready') open();
        });
    }
    document.addEventListener('webOSRelaunch', open);
})();
