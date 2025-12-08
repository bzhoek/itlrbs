
Update ratings from Music to rekordbox and add tags based on playlist membership.

### Tagging

For each song in named playlists of Music library
- add playlist name as tag to rekordbox track

### Rating

For all songs in Music library
1. Check if file exists
2. Delete if Music rating is 1-star
3. Extract track id from between `[]` of `2020 Souls -- Aaaron [918205852].mp3`
4. Find in rekordbox on track id in between `[]`
5. Update rekordbox if it has 0-rating, write ID3 tags of file
   - rating to popularity tag
   - WWYY-weekstamp in group tag
6. Log discrepancy between Music and rekordbox rating
   - append to group ID3 tag as '^RB' to identify after refresh in Music