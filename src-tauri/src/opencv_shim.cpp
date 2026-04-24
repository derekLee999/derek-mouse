#include <opencv2/imgproc.hpp>
#include <cstdint>

extern "C" {

struct CvMatchResult {
    double score;
    int x;
    int y;
    int found;
};

CvMatchResult match_template_opencv(
    const uint8_t* search_data, int search_w, int search_h,
    const uint8_t* template_data, int template_w, int template_h,
    double threshold
) {
    CvMatchResult res = {0.0, 0, 0, 0};

    try {
        if (!search_data || !template_data ||
            search_w <= 0 || search_h <= 0 ||
            template_w <= 0 || template_h <= 0 ||
            template_w > search_w || template_h > search_h) {
            return res;
        }

        cv::Mat search(search_h, search_w, CV_8UC1, const_cast<uint8_t*>(search_data));
        cv::Mat templ(template_h, template_w, CV_8UC1, const_cast<uint8_t*>(template_data));

        cv::Mat result;
        cv::matchTemplate(search, templ, result, cv::TM_CCOEFF_NORMED);

        double minVal, maxVal;
        cv::Point minLoc, maxLoc;
        cv::minMaxLoc(result, &minVal, &maxVal, &minLoc, &maxLoc);

        res.score = maxVal * 100.0;
        res.x = maxLoc.x + template_w / 2;
        res.y = maxLoc.y + template_h / 2;
        res.found = maxVal >= threshold ? 1 : 0;
    } catch (...) {
        // ignore all exceptions, return not found
    }

    return res;
}

}
